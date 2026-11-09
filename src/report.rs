//! The result of one run: the two ends of the Change Set, the Global Files in it and the
//! Affected Crates among the workspace members, with everything an output format needs
//! to explain why. Rendering lives in `render`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Serialize, Serializer};

use crate::affected::Reason;
use crate::mapping::FileOutcome;

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
    /// Changed non-member path crates, sorted by name. Tree only: never serialised, since
    /// only members are output in the other formats.
    #[serde(skip)]
    pub changed_non_members: Vec<String>,
    /// Crate name -> its direct Dependents among the affected members, sorted by name;
    /// crates without one are absent, and the whole map is empty with `changed_only`.
    /// Tree only: the inverse of `via`, so the tree can be walked downward from a root
    /// without the graph.
    #[serde(skip)]
    pub dependents: BTreeMap<String, Vec<String>>,
    /// Where each file of the Change Set under the workspace root landed, sorted by file;
    /// what `-v` explains on stderr. Never serialised: it is a diagnostic, not a result.
    #[serde(skip)]
    pub file_outcomes: Vec<FileOutcome>,
}

/// The inspected end of the Change Set: a commit, or the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    /// The commit `--to` resolved to.
    Commit(String),
    /// The working tree, including staged, unstaged and untracked files.
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
    /// The package name.
    pub name: String,
    /// Absolute, canonical path of the crate's `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// The single reason of highest precedence, see [`Reason`].
    pub reason: Reason,
    /// The files of the Change Set that mapped to this crate, by its directory or an Extra
    /// Path, relative to the workspace root and sorted. Filled whatever the reason.
    pub changed_files: Vec<PathBuf>,
    /// The direct dependencies through which affectedness arrived, sorted by name; empty
    /// unless the reason is `dependent`.
    pub via: Vec<String>,
}

impl AffectedCrate {
    /// Whether the crate is a Changed Crate: its own files are in the Change Set. This
    /// holds whatever `reason` reports, since a Global File takes precedence there.
    pub fn is_changed(&self) -> bool {
        !self.changed_files.is_empty()
    }
}
