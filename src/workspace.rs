//! Loads the Cargo workspace through `cargo metadata`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use cargo_metadata::{Metadata, MetadataCommand};

use crate::graph::DependencyGraph;
use crate::mapping::CrateDir;

pub struct Workspace {
    /// Canonical path of the workspace root.
    pub root: PathBuf,
    /// Workspace members and the non-member path dependencies reachable from them.
    pub packages: Vec<Package>,
    /// Dependencies between the packages, by name.
    pub graph: DependencyGraph,
    /// User globs from `[workspace.metadata.affected].global`, relative to the root.
    pub global_globs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    /// Canonical directory holding the manifest.
    pub dir: PathBuf,
    /// Whether the package is a workspace member; only members are ever output.
    pub member: bool,
}

impl Workspace {
    /// Loads the workspace of the given manifest, or of the current directory.
    pub fn load(manifest_path: Option<&Path>) -> Result<Self> {
        let mut cmd = MetadataCommand::new();
        if let Some(path) = manifest_path {
            cmd.manifest_path(path);
        }
        let metadata = cmd
            .exec()
            .context("loading the workspace with `cargo metadata`")?;
        let root = metadata
            .workspace_root
            .canonicalize()
            .context("canonicalising the workspace root")?;

        // Path packages are those without a source (registry, git); they are the nodes.
        let path_packages: Vec<_> = metadata
            .packages
            .iter()
            .filter(|pkg| pkg.source.is_none())
            .collect();
        let packages = path_packages
            .iter()
            .map(|pkg| {
                Ok(Package {
                    name: pkg.name.to_string(),
                    dir: canonical_dir(
                        pkg.manifest_path
                            .parent()
                            .expect("manifest path has a parent")
                            .as_std_path(),
                        &pkg.name,
                    )?,
                    member: metadata.workspace_members.contains(&pkg.id),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Edges come from the declared dependencies, not the feature-resolved graph, so
        // optional and target-specific dependencies count regardless of feature state.
        let name_by_dir: HashMap<&Path, &str> = packages
            .iter()
            .map(|p| (p.dir.as_path(), p.name.as_str()))
            .collect();
        let mut graph = DependencyGraph::default();
        for pkg in &path_packages {
            for dep in &pkg.dependencies {
                let Some(dep_path) = &dep.path else { continue };
                let dep_dir = canonical_dir(dep_path.as_std_path(), &dep.name)?;
                if let Some(dependency) = name_by_dir.get(dep_dir.as_path()) {
                    graph.add_dependency(&pkg.name, dependency);
                }
            }
        }

        let global_globs = global_globs(&metadata)?;

        Ok(Self {
            root,
            packages,
            graph,
            global_globs,
        })
    }

    /// The packages as crate directories relative to the workspace root, the shape the
    /// mapping works on. A package outside the root keeps its absolute path and
    /// therefore never matches a workspace-relative file.
    pub fn crate_dirs(&self) -> Vec<CrateDir> {
        self.packages
            .iter()
            .map(|p| {
                let dir = p.dir.strip_prefix(&self.root).unwrap_or(&p.dir);
                CrateDir::new(&p.name, dir)
            })
            .collect()
    }

    pub fn is_member(&self, name: &str) -> bool {
        self.packages.iter().any(|p| p.member && p.name == name)
    }

    /// Names of the workspace members.
    pub fn member_names(&self) -> Vec<String> {
        self.packages
            .iter()
            .filter(|p| p.member)
            .map(|p| p.name.clone())
            .collect()
    }
}

/// The `global` list under `[workspace.metadata.affected]`, if any.
fn global_globs(metadata: &Metadata) -> Result<Vec<String>> {
    let Some(globs) = metadata.workspace_metadata.pointer("/affected/global") else {
        return Ok(Vec::new());
    };
    globs
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| {
            anyhow!("`[workspace.metadata.affected].global` must be an array of strings")
        })
}

fn canonical_dir(dir: &Path, name: &str) -> Result<PathBuf> {
    dir.canonicalize()
        .with_context(|| format!("canonicalising the directory of `{name}`"))
}
