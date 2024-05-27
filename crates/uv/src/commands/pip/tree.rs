use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use anyhow::Result;
use owo_colors::OwoColorize;
use petgraph::algo::toposort;
use tracing::debug;

use distribution_types::{Diagnostic, InstalledDist, Name};
use uv_cache::Cache;
use uv_configuration::PreviewMode;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::{PythonEnvironment, SystemPython};
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Display the installed packages in the current environment as a dependency tree.
pub(crate) fn pip_tree(
    strict: bool,
    python: Option<&str>,
    system: bool,
    preview: PreviewMode,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let system = if system {
        SystemPython::Required
    } else {
        SystemPython::Allowed
    };
    let venv = PythonEnvironment::find(python, system, preview, cache)?;

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;
    match DisplayDependencyGraph::new(&site_packages, printer).render() {
        Ok(()) => {}
        Err(e) => {
            writeln!(printer.stderr(), "{}", e.to_string().red().bold())?;
            return Ok(ExitStatus::Error);
        }
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

// Render the line for the given installed distribution in the dependency tree.
fn render_line(installed_dist: &InstalledDist, indent: usize) -> String {
    let mut line = String::new();
    if indent > 0 {
        line.push_str("    ".repeat(indent - 1).as_str());
        line.push_str("└──");
        line.push(' ');
    }
    line.push_str((*installed_dist.name()).to_string().as_str());
    line.push_str(" v");
    line.push_str((*installed_dist.version()).to_string().as_str());
    line
}
#[derive(Debug)]
struct DisplayDependencyGraph<'a> {
    // Dependency graph of installed distributions.
    graph: petgraph::prelude::Graph<&'a InstalledDist, ()>,
    // Map from package name to node index in the graph.
    package_index: HashMap<&'a PackageName, petgraph::prelude::NodeIndex>,
    printer: Printer,
}

impl<'a> DisplayDependencyGraph<'a> {
    /// Create a new [`DisplayDependencyGraph`] for the given graph.
    fn new(site_packages: &'a SitePackages, printer: Printer) -> DisplayDependencyGraph<'a> {
        let mut graph = petgraph::prelude::Graph::<&InstalledDist, ()>::new();
        let mut package_index = HashMap::new();

        for site_package in site_packages.iter() {
            package_index.insert(site_package.name(), graph.add_node(site_package));
        }
        for site_package in site_packages.iter() {
            for required in site_package.metadata().unwrap().requires_dist {
                if let Some(req) = package_index.get(&required.name) {
                    graph.add_edge(*req, package_index[&site_package.name()], ());
                }
            }
        }
        Self {
            graph,
            package_index,
            printer,
        }
    }

    // Visit and print the given installed distribution and those required by it.
    fn visit(&self, installed_dist: &InstalledDist, indent: usize, visited: &mut HashSet<String>) {
        if visited.contains(&installed_dist.name().to_string()) {
            return;
        }
        let line = render_line(installed_dist, indent);
        writeln!(self.printer.stdout(), "{line}").unwrap();
        visited.insert(installed_dist.name().to_string());
        for required in installed_dist.metadata().unwrap().requires_dist {
            match self.package_index.get(&required.name) {
                Some(index) => {
                    self.visit(self.graph[*index], indent + 1, visited);
                }
                None => continue,
            }
        }
    }

    // Recursively visit the topologically sorted list of installed distributions.
    fn render(&self) -> Result<(), anyhow::Error> {
        let mut visited: HashSet<String> = HashSet::new();
        match toposort(&self.graph, None) {
            Ok(sorted) => {
                for node_index in sorted.iter().rev() {
                    self.visit(self.graph[*node_index], 0, &mut visited);
                }
                Ok(())
            }
            // TODO: Improve the error handling for cyclic dependency;
            //       offending packages' names should be printed to stderr.
            Err(_) => Err(anyhow::anyhow!(
                "Failed to print the dependency tree due to cyclic dependency."
            )),
        }
    }
}
