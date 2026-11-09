//! The result of one run: the two ends of the Change Set, the Global Files in it and the
//! Affected Crates among the workspace members, with everything an output format needs
//! to explain why. Rendering lives in `render`.

use std::path::PathBuf;

use serde::{Serialize, Serializer};

use crate::affected::Reason;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    /// The commit the Base resolved to.
    pub base: String,
    /// The inspected end of the Change Set.
    pub to: To,
    /// The Global Files in the Change Set, relative to the workspace root, sorted.
    pub global_files: Vec<PathBuf>,
    /// The Affected Crates that are workspace members, sorted by name.
    pub affected: Vec<AffectedCrate>,
}

/// The inspected end of the Change Set: a commit, or the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    Commit(String),
    Worktree,
}

impl To {
    /// The commit hash, or `None` for the working tree.
    pub fn commit(&self) -> Option<&str> {
        match self {
            Self::Commit(hash) => Some(hash),
            Self::Worktree => None,
        }
    }
}

/// Serialises as the commit hash or the string `worktree`.
impl Serialize for To {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.commit().unwrap_or("worktree"))
    }
}

/// A workspace member that is an Affected Crate, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AffectedCrate {
    pub name: String,
    /// Absolute path of the crate's `Cargo.toml`.
    pub manifest_path: PathBuf,
    pub reason: Reason,
    /// The files of the Change Set that mapped to this crate, by its directory or an Extra
    /// Path, relative to the workspace root and sorted. Filled whatever the reason.
    pub changed_files: Vec<PathBuf>,
    /// The direct dependencies through which affectedness arrived, sorted by name; empty
    /// unless the reason is `dependent`.
    pub via: Vec<String>,
}
