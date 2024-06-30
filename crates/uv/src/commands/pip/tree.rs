use distribution_types::{Diagnostic, InstalledDist, Name};
use owo_colors::OwoColorize;
use pep508_rs::MarkerEnvironment;
use pypi_types::VerbatimParsedUrl;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use tracing::debug;
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_toolchain::EnvironmentPreference;
use uv_toolchain::PythonEnvironment;
use uv_toolchain::ToolchainRequest;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Display the installed packages in the current environment as a dependency tree.
pub(crate) fn pip_tree(
    depth: u8,
    prune: Vec<PackageName>,
    no_dedupe: bool,
    invert: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    _preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> anyhow::Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(ToolchainRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    debug!(
        "Using Python {} environment at {}",
        environment.interpreter().python_version(),
        environment.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;

    let rendered_tree = DisplayDependencyGraph::new(
        &site_packages,
        depth.into(),
        prune,
        no_dedupe,
        invert,
        environment.interpreter().markers(),
    )
    .render()
    .join("\n");
    writeln!(printer.stdout(), "{rendered_tree}").unwrap();
    if rendered_tree.contains('*') {
        let message = if no_dedupe {
            "(*) Package tree is a cycle and cannot be shown".italic()
        } else {
            "(*) Package tree already displayed".italic()
        };
        writeln!(printer.stdout(), "{message}")?;
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }
    Ok(ExitStatus::Success)
}

/// Filter out all required packages of the given distribution if they
/// are required by an extra.
///
/// For example, `requests==2.32.3` requires `charset-normalizer`, `idna`, `urllib`, and `certifi` at
/// all times, `PySocks` on `socks` extra and `chardet` on `use_chardet_on_py3` extra.
/// This function will return `["charset-normalizer", "idna", "urllib", "certifi"]` for `requests`.
fn required_with_no_extra(
    dist: &InstalledDist,
    markers: &MarkerEnvironment,
) -> Vec<pep508_rs::Requirement<VerbatimParsedUrl>> {
    let metadata = dist.metadata().unwrap();
    return metadata
        .requires_dist
        .into_iter()
        .filter(|requirement| {
            requirement
                .marker
                .as_ref()
                .map_or(true, |m| m.evaluate(markers, &[]))
        })
        .collect::<Vec<_>>();
}

#[derive(Debug)]
struct DisplayDependencyGraph<'a> {
    site_packages: &'a SitePackages,
    /// Map from package name to the installed distribution.
    dist_by_package_name: HashMap<&'a PackageName, &'a InstalledDist>,
    /// Maximum display depth of the dependency tree
    depth: usize,
    /// Prune the given package from the display of the dependency tree.
    prune: Vec<PackageName>,
    /// Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,

    /// Map from package name to the list of required (reversed if --invert is given) packages.
    requires_map: HashMap<PackageName, Vec<PackageName>>,
}

impl<'a> DisplayDependencyGraph<'a> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    fn new(
        site_packages: &'a SitePackages,
        depth: usize,
        prune: Vec<PackageName>,
        no_dedupe: bool,
        invert: bool,
        markers: &'a MarkerEnvironment,
    ) -> DisplayDependencyGraph<'a> {
        let mut dist_by_package_name = HashMap::new();
        let mut requires_map = HashMap::new();

        for site_package in site_packages.iter() {
            dist_by_package_name.insert(site_package.name(), site_package);
        }
        for site_package in site_packages.iter() {
            for required in required_with_no_extra(site_package, markers) {
                if invert {
                    requires_map
                        .entry(required.name.clone())
                        .or_insert_with(Vec::new)
                        .push(site_package.name().clone());
                } else {
                    requires_map
                        .entry(site_package.name().clone())
                        .or_insert_with(Vec::new)
                        .push(required.name.clone());
                }
            }
        }

        Self {
            site_packages,
            dist_by_package_name,
            depth,
            prune,
            no_dedupe,
            requires_map,
        }
    }

    /// Perform a depth-first traversal of the given distribution and its dependencies.
    fn visit(
        &self,
        installed_dist: &InstalledDist,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Vec<String> {
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        let package_name = installed_dist.name().to_string();
        let is_visited = visited.contains(&package_name);
        let line = format!("{} v{}", package_name, installed_dist.version());

        // Skip the traversal if
        // 1. the package is in the current traversal path (i.e. a dependency cycle)
        // 2. if the package has been visited and de-duplication is enabled (default)
        if path.contains(&package_name) || (is_visited && !self.no_dedupe) {
            return vec![format!("{} (*)", line)];
        }

        let mut lines = vec![line];
        let empty_vec = Vec::new();
        path.push(package_name.clone());
        visited.insert(package_name.clone());
        let required_packages = self
            .requires_map
            .get(installed_dist.name())
            .unwrap_or(&empty_vec)
            .iter()
            .filter(|p| {
                // Skip if the current package is not one of the installed distributions.
                self.dist_by_package_name.contains_key(p) && !self.prune.contains(*p)
            })
            .collect::<Vec<_>>();
        for (index, required_package) in required_packages.iter().enumerate() {
            // For sub-visited packages, add the prefix to make the tree display user-friendly.
            // The key observation here is you can group the tree as follows when you're at the
            // root of the tree:
            // root_package
            // ├── level_1_0          // Group 1
            // │   ├── level_2_0      ...
            // │   │   ├── level_3_0  ...
            // │   │   └── level_3_1  ...
            // │   └── level_2_1      ...
            // ├── level_1_1          // Group 2
            // │   ├── level_2_2      ...
            // │   └── level_2_3      ...
            // └── level_1_2          // Group 3
            //     └── level_2_4      ...
            //
            // The lines in Group 1 and 2 have `├── ` at the top and `|   ` at the rest while
            // those in Group 3 have `└── ` at the top and `    ` at the rest.
            // This observation is true recursively even when looking at the subtree rooted
            // at `level_1_0`.
            let (prefix_top, prefix_rest) = if required_packages.len() - 1 == index {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };

            let mut prefixed_lines = Vec::new();
            for (visited_index, visited_line) in self
                .visit(self.dist_by_package_name[required_package], visited, path)
                .iter()
                .enumerate()
            {
                prefixed_lines.push(format!(
                    "{}{}",
                    if visited_index == 0 {
                        prefix_top
                    } else {
                        prefix_rest
                    },
                    visited_line
                ));
            }
            lines.extend(prefixed_lines);
        }
        path.pop();
        lines
    }

    // Depth-first traverse the nodes to render the tree.
    // The starting nodes are the ones without incoming edges.
    fn render(&self) -> Vec<String> {
        // The starting nodes are those that are not required by any other package.
        let mut non_starting_nodes = HashSet::new();
        for children in self.requires_map.values() {
            non_starting_nodes.extend(children);
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut lines: Vec<String> = Vec::new();
        for site_package in self.site_packages.iter() {
            // If the current package is not required by any other package, start the traversal
            // with the current package as the root.
            if !non_starting_nodes.contains(site_package.name()) {
                lines.extend(self.visit(site_package, &mut visited, &mut Vec::new()));
            }
        }
        lines
    }
}
