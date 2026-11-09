//! Loads the Cargo workspace through `cargo metadata`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use cargo_metadata::MetadataCommand;
use serde_json::Value;

use crate::graph::DependencyGraph;
use crate::mapping::{CrateDir, GlobalFiles, crate_setting};

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
    /// The crate's `[package.metadata.affected]` table.
    pub affected: CrateMetadata,
}

/// What a crate declares in `[package.metadata.affected]`; globs relative to its directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrateMetadata {
    /// Extra Paths: globs, possibly starting with `../`, that make the crate Changed.
    pub extra_paths: Vec<String>,
    /// Globs for files inside the crate's directory that do not make it a Changed Crate.
    pub ignore: Vec<String>,
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
                    affected: crate_metadata(&pkg.name, &pkg.metadata)?,
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

        let global_globs = string_list(&metadata.workspace_metadata, "global", || {
            GlobalFiles::SETTING.to_string()
        })?;

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
    pub fn crate_dirs(&self) -> Result<Vec<CrateDir>> {
        self.packages
            .iter()
            .map(|p| {
                let dir = p.dir.strip_prefix(&self.root).unwrap_or(&p.dir);
                CrateDir::new(&p.name, dir)
                    .extra_paths(&p.affected.extra_paths)?
                    .ignore(&p.affected.ignore)
            })
            .collect()
    }

    /// The workspace member called `name`, if there is one.
    pub fn member(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.member && p.name == name)
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

/// The `[package.metadata.affected]` table of crate `name`, given the package metadata.
fn crate_metadata(name: &str, metadata: &Value) -> Result<CrateMetadata> {
    Ok(CrateMetadata {
        extra_paths: string_list(metadata, "extra-paths", || {
            crate_setting(name, "extra-paths")
        })?,
        ignore: string_list(metadata, "ignore", || crate_setting(name, "ignore"))?,
    })
}

/// The string list at `affected.<key>` of a metadata table, or empty when absent.
/// `origin` names the setting in the error for a value of the wrong type.
fn string_list(
    metadata: &Value,
    key: &str,
    origin: impl FnOnce() -> String,
) -> Result<Vec<String>> {
    let Some(value) = metadata.pointer(&format!("/affected/{key}")) else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| anyhow!("{} must be an array of strings", origin()))
}

fn canonical_dir(dir: &Path, name: &str) -> Result<PathBuf> {
    dir.canonicalize()
        .with_context(|| format!("canonicalising the directory of `{name}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_crate_without_an_affected_table_declares_nothing() {
        let got = crate_metadata(
            "plain",
            &json!({ "docs": { "rs": { "all-features": true } } }),
        );
        assert_eq!(got.unwrap(), CrateMetadata::default());
    }

    #[test]
    fn extra_paths_and_ignore_are_read_from_the_affected_table() {
        let got = crate_metadata(
            "codegen",
            &json!({ "affected": { "extra-paths": ["../proto/**"], "ignore": ["README.md"] } }),
        );
        assert_eq!(
            got.unwrap(),
            CrateMetadata {
                extra_paths: vec!["../proto/**".to_string()],
                ignore: vec!["README.md".to_string()],
            }
        );
    }

    #[test]
    fn a_setting_of_the_wrong_type_is_an_error_naming_the_crate_and_the_setting() {
        let err =
            crate_metadata("broken", &json!({ "affected": { "extra-paths": "x" } })).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("`broken`"), "{msg}");
        assert!(msg.contains("extra-paths"), "{msg}");

        let err = crate_metadata("broken", &json!({ "affected": { "ignore": [1] } })).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("`broken`"), "{msg}");
        assert!(msg.contains("ignore"), "{msg}");
    }
}
