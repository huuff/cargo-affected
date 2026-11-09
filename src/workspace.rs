//! Loads the Cargo workspace through `cargo metadata`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;

use crate::mapping::CrateDir;

pub struct Workspace {
    /// Canonical path of the workspace root.
    pub root: PathBuf,
    /// Workspace members, each with the canonical directory holding its manifest.
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub dir: PathBuf,
}

impl Workspace {
    /// Loads the workspace of the given manifest, or of the current directory.
    pub fn load(manifest_path: Option<&Path>) -> Result<Self> {
        let mut cmd = MetadataCommand::new();
        cmd.no_deps();
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
        let members = metadata
            .workspace_packages()
            .into_iter()
            .map(|pkg| {
                let dir = pkg
                    .manifest_path
                    .parent()
                    .expect("manifest path has a parent")
                    .canonicalize()
                    .with_context(|| format!("canonicalising the directory of `{}`", pkg.name))?;
                Ok(Member {
                    name: pkg.name.to_string(),
                    dir,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { root, members })
    }

    /// The members as crate directories relative to the workspace root, the shape
    /// the mapping works on. A member outside the root keeps its absolute path and
    /// therefore never matches a workspace-relative file.
    pub fn crate_dirs(&self) -> Vec<CrateDir> {
        self.members
            .iter()
            .map(|m| {
                let dir = m.dir.strip_prefix(&self.root).unwrap_or(&m.dir);
                CrateDir::new(&m.name, dir)
            })
            .collect()
    }
}
