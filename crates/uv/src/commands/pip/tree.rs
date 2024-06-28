use std::collections::BTreeSet;
use std::fmt::Write;

use distribution_types::{Diagnostic, InstalledDist, Name};
use owo_colors::OwoColorize;
use pep508_rs::ExtraName;
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
use std::collections::{HashMap, HashSet};

/// Display the installed packages in the current environment as a dependency tree.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pip_tree(
    depth: u8,
    prune: Vec<PackageName>,
    no_dedupe: bool,
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

    let rendered_tree = DisplayDependencyGraph::new(&site_packages, depth.into(), prune, no_dedupe)
        .render()
        .join("\n");
    writeln!(printer.stdout(), "{rendered_tree}").unwrap();
    if rendered_tree.contains('*') {
        writeln!(
            printer.stdout(),
            "{}",
            "(*) Package tree already displayed".italic()
        )?;
    }
    if rendered_tree.contains('#') {
        writeln!(printer.stdout(), "{}", "(#) Dependency cycle".italic())?;
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

#[derive(Debug)]
struct DisplayDependencyGraph<'a> {
    site_packages: &'a SitePackages,

    // Map from package name to the installed distribution.
    dist_by_package_name: HashMap<&'a PackageName, &'a InstalledDist>,

    // Set of package names that are required by at least one installed distribution.
    // It is used to determine the starting nodes when recursing the
    // dependency graph.
    required_packages: HashSet<PackageName>,

    // Maximum display depth of the dependency tree
    depth: usize,

    // Prune the given package from the display of the dependency tree.
    prune: Vec<PackageName>,

    // Whether to de-duplicate the displayed dependencies.
    no_dedupe: bool,
}

impl<'a> DisplayDependencyGraph<'a> {
    /// Create a new [`DisplayDependencyGraph`] for the set of installed distributions.
    fn new(
        site_packages: &'a SitePackages,
        depth: usize,
        prune: Vec<PackageName>,
        no_dedupe: bool,
    ) -> DisplayDependencyGraph<'a> {
        let mut dist_by_package_name = HashMap::new();
        let mut required_packages = HashSet::new();
        for site_package in site_packages.iter() {
            dist_by_package_name.insert(site_package.name(), site_package);
        }
        for site_package in site_packages.iter() {
            for required in site_package
                .metadata()
                .unwrap()
                .requires_dist
                .into_iter()
                .filter(|d| dist_by_package_name.contains_key(&d.name))
                .collect::<Vec<_>>()
            {
                required_packages.insert(required.name.clone());
            }
        }

        Self {
            site_packages,
            dist_by_package_name,
            required_packages,
            depth,
            prune,
            no_dedupe,
        }
    }

    // Depth-first traversal of the given distribution and its dependencies.
    fn visit(
        &self,
        installed_dist: &InstalledDist,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        extra: Option<String>,
    ) -> Vec<String> {
        // println!(
        //     "visit {} {} {}",
        //     installed_dist.name(),
        //     installed_dist.version(),
        //     extra.clone().unwrap_or("".to_string())
        // );
        // Short-circuit if the current path is longer than the provided depth.
        if path.len() > self.depth {
            return Vec::new();
        }

        // Short-circuit if the current package is given in the prune list.
        if self.prune.contains(installed_dist.name()) {
            return Vec::new();
        }

        let package_name = installed_dist.name().to_string();
        let line = format!(
            "{}{} v{}",
            if let Some(extra_name) = extra {
                format!("[{}] ", extra_name)
            } else {
                "".to_string()
            },
            package_name,
            installed_dist.version()
        );

        if path.contains(&package_name) {
            return vec![format!("{} (#)", line)];
        }

        // If the package has been visited and de-duplication is enabled (default),
        // skip the traversal.
        if visited.contains(&package_name) && !self.no_dedupe {
            return vec![format!("{} (*)", line)];
        }

        let mut lines = vec![line];

        path.push(package_name.clone());
        visited.insert(package_name.clone());

        // Need to be able to remove the extras that have already been used
        // in the event that there are two distinct extras sharing the same dependency.
        let mut used_extras: BTreeSet<ExtraName> = BTreeSet::new();
        let extras = installed_dist.metadata().unwrap().provides_extras;
        println!("provided extras: {:?}", extras);
        let required_packages = installed_dist
            .metadata()
            .unwrap()
            .requires_dist
            .into_iter()
            .filter(|d| {
                self.dist_by_package_name.contains_key(&d.name) // must be an installed distribution and
                                                                && (d.marker.is_none() // either always required or
                                                                    || d // required by an extra
                                                                        .marker
                                                                        .as_ref()
                                                                        .unwrap()
                                                                        .evaluate_optional_environment(None, &extras.as_slice()))
            })
            .collect::<Vec<_>>();

        for (index, required_package) in required_packages.iter().enumerate() {
            let extra_index = extras.iter().position(|e| {
                required_package.marker.is_some()
                    && required_package
                        .marker
                        .as_ref()
                        .unwrap()
                        .evaluate_optional_environment(None, &[e.clone()])
                    && !used_extras.contains(e)
            });
            if extra_index.is_some() {
                used_extras.insert(extras[extra_index.unwrap()].clone());
            }

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
                .visit(
                    self.dist_by_package_name[&required_package.name],
                    visited,
                    path,
                    if extra_index.is_some() {
                        Some(extras[extra_index.unwrap()].to_string())
                    } else {
                        None
                    },
                )
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
        let mut visited: HashSet<String> = HashSet::new();
        let mut lines: Vec<String> = Vec::new();
        for site_package in self.site_packages.iter() {
            // If the current package is not required by any other package, start the traversal
            // with the current package as the root.
            if !self.required_packages.contains(site_package.name()) {
                lines.extend(self.visit(site_package, &mut visited, &mut Vec::new(), None));
            }
        }
        lines
    }
}
