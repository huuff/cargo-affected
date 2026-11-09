//! Decides which workspace crates CI needs to run for, given what changed in git.
//!
//! Vocabulary (see `CONTEXT.md`): the *Change Set* is the set of files git reports as
//! different between the *Base* and the inspected state; a *Changed Crate* is a workspace
//! member whose own files appear in the Change Set; an *Affected Crate* is a Changed Crate
//! or a workspace member that is a *Dependent* of one.

pub mod git;
pub mod graph;
pub mod mapping;
pub mod workspace;

use std::path::PathBuf;

use anyhow::{Context, Result};

use git::{Base, Repo};
use workspace::Workspace;

/// What to compute the Change Set over, and how to report the result.
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Branch whose merge-base with HEAD is the Base; `None` tries the default branches.
    pub base: Option<String>,
    /// Exact ref to use as the Base; takes precedence over `base`.
    pub from: Option<String>,
    /// Ref of the inspected state; `None` inspects the working tree.
    pub to: Option<String>,
    pub manifest_path: Option<PathBuf>,
    /// Report only Changed Crates, not the crates affected merely as Dependents.
    pub changed_only: bool,
}

/// Names of the Affected Crates that are workspace members, sorted. With
/// `changed_only`, just the Changed Crates.
pub fn affected_crates(opts: &Options) -> Result<Vec<String>> {
    let workspace = Workspace::load(opts.manifest_path.as_deref())?;
    let repo = Repo::discover(&workspace.root)?;
    let base = match &opts.from {
        Some(rev) => Base::Exact(rev.clone()),
        None => Base::MergeBase(opts.base.clone()),
    };
    let from = repo.resolve_base(&base)?;
    let change_set = repo.change_set(&from, opts.to.as_deref())?;

    let workspace_in_repo = workspace
        .root
        .strip_prefix(&repo.root)
        .context("the workspace root is not inside the git repository")?;
    let files = change_set.files_under(workspace_in_repo);
    let changed = mapping::changed_crates(&workspace.crate_dirs(), files);
    let affected = if opts.changed_only {
        changed
    } else {
        workspace.graph.affected(changed)
    };
    Ok(affected
        .into_iter()
        .filter(|name| workspace.is_member(name))
        .collect())
}
