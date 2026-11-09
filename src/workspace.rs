//! Loads the Cargo workspace through `cargo metadata`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

use anyhow::{Context, Result, anyhow, bail};
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
    /// The canonical root of the workspace of the given manifest, or of the current
    /// directory, found with `cargo locate-project`. Unlike [`Workspace::load`] this reads
    /// no dependency graph and therefore never rewrites a stale `Cargo.lock`, so it is
    /// safe to call before the Change Set is computed.
    pub fn locate(manifest_path: Option<&Path>) -> Result<PathBuf> {
        let mut cmd = Command::new(cargo());
        cmd.args(["locate-project", "--workspace", "--message-format", "plain"]);
        if let Some(path) = manifest_path {
            cmd.arg("--manifest-path").arg(path);
        }
        let out = cmd.output().context("running `cargo locate-project`")?;
        if !out.status.success() {
            let reason = String::from_utf8_lossy(&out.stderr);
            let reason = reason.trim();
            bail!(
                "no Cargo workspace found: {}",
                reason.strip_prefix("error: ").unwrap_or(reason)
            );
        }
        let manifest = String::from_utf8(out.stdout).context("cargo output is not UTF-8")?;
        Path::new(manifest.trim_end())
            .parent()
            .context("the workspace manifest path has no parent")?
            .canonicalize()
            .context("canonicalising the workspace root")
    }

    /// Loads the workspace rooted at `root` (from [`Workspace::locate`]) through
    /// `cargo metadata` for the given manifest, or the current directory's. `cargo
    /// metadata` rewrites a stale `Cargo.lock` and creates a missing one; the lockfile at
    /// `root` is put back exactly as it was, so a run leaves the tree as it found it and
    /// the next run computes the same Change Set.
    pub fn load(root: &Path, manifest_path: Option<&Path>) -> Result<Self> {
        let mut cmd = MetadataCommand::new();
        if let Some(path) = manifest_path {
            cmd.manifest_path(path);
        }
        let lockfile = Lockfile::snapshot(&root.join("Cargo.lock"))?;
        let metadata = cmd.exec();
        lockfile.restore()?;
        let metadata = metadata.context("loading the workspace with `cargo metadata`")?;
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

    /// Whether `name` is a workspace member.
    pub fn is_member(&self, name: &str) -> bool {
        self.member(name).is_some()
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

/// The bytes of a lockfile, or its absence, taken before `cargo metadata` runs so that
/// whatever it writes can be undone.
struct Lockfile {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

impl Lockfile {
    fn snapshot(path: &Path) -> Result<Self> {
        let contents = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(err).with_context(|| format!("reading {}", path.display()));
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            contents,
        })
    }

    /// Puts the file back as it was, touching it only if `cargo metadata` changed it.
    fn restore(self) -> Result<()> {
        let now = Self::snapshot(&self.path)?.contents;
        if now == self.contents {
            return Ok(());
        }
        match self.contents {
            Some(bytes) => fs::write(&self.path, bytes)
                .with_context(|| format!("restoring {}", self.path.display())),
            None => fs::remove_file(&self.path)
                .with_context(|| format!("removing {}", self.path.display())),
        }
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

/// The `cargo` to run: the one that dispatched us when invoked as `cargo affected`, as
/// `cargo_metadata` also does, else the one on `PATH`.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
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
