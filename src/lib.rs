//! Decides which workspace crates CI needs to run for, given what changed in git.
//!
//! Vocabulary (see `CONTEXT.md`): the *Change Set* is the set of files git reports as
//! different between the *Base* and the inspected state; a *Changed Crate* is a workspace
//! member whose own files appear in the Change Set; an *Affected Crate* is a Changed Crate
//! or a workspace member that is a *Dependent* of one; a *Global File* is a file outside
//! any crate, such as the workspace manifest, whose change makes every member an Affected
//! Crate; an *Extra Path* is a path outside a crate's directory that the crate declares as
//! its own; a *Locked Dependency* is a third-party package the lockfile pins for a crate,
//! and a change to one makes the crates using it Changed Crates.

pub mod affected;
pub mod git;
pub mod graph;
pub mod lockfile;
pub mod mapping;
mod process;
pub mod render;
pub mod report;
pub mod workspace;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub use affected::Reason;
use git::{Base, Repo};
pub use report::{AffectedCrate, Report, To};
use workspace::Workspace;

/// What to compute the Change Set over, and how to report the result.
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Branch whose merge-base with HEAD is the Base; `None` detects the default branch.
    pub base: Option<String>,
    /// Exact ref to use as the Base; takes precedence over `base`.
    pub from: Option<String>,
    /// Ref of the inspected state; `None` inspects the working tree.
    pub to: Option<String>,
    pub manifest_path: Option<PathBuf>,
    /// Report only Changed Crates, not the crates affected merely as Dependents.
    pub changed_only: bool,
}

/// Computes the Change Set between the Base and the inspected state and reports the
/// Affected Crates that are workspace members, sorted by name. With `changed_only`, the
/// crates affected merely as Dependents are left out.
pub fn report(opts: &Options) -> Result<Report> {
    // The Change Set is computed before the workspace is loaded: `cargo metadata` rewrites
    // a stale `Cargo.lock`, which must not then show up as a changed Global File (the
    // rewrite is undone afterwards, but the order keeps this run honest too).
    let workspace_root = Workspace::locate(opts.manifest_path.as_deref())?;
    let repo = Repo::discover(&workspace_root)?;
    let base = match &opts.from {
        Some(rev) => Base::Exact(rev.clone()),
        None => Base::MergeBase(opts.base.clone()),
    };
    let base = repo.resolve_base(&base)?;
    let to = match &opts.to {
        Some(rev) => To::Commit(repo.rev_parse(rev)?),
        None => To::Worktree,
    };
    let change_set = repo.change_set(&base, to.commit())?;
    let workspace_in_repo = workspace_root
        .strip_prefix(&repo.root)
        .context("the workspace root is not inside the git repository")?;
    // Read before the workspace is loaded as well, for the same reason.
    let locked_dependencies =
        changed_locked_dependencies(&repo, &change_set, workspace_in_repo, &base, &to)?;

    let workspace = Workspace::load(&workspace_root, opts.manifest_path.as_deref())?;
    let files = change_set.files_relative_to(workspace_in_repo);
    let mut globals = mapping::GlobalFiles::new(&workspace.global_globs)?;
    if let Some(locked_dependencies) = locked_dependencies {
        globals = globals.narrow_lockfile(locked_dependencies);
    }
    let mapping = mapping::map_files(&workspace.crate_dirs()?, &globals, files);

    // A Global File makes every member affected; the mapping has already kept such
    // files away from the crates, so a root package is not Changed by them.
    let global_files = mapping.global_files();
    let global = if global_files.is_empty() {
        Vec::new()
    } else {
        workspace.member_names()
    };
    let changed = mapping.changed();
    let affected = affected::affected_crates(global, changed.iter().cloned(), &workspace.graph);
    // The tree walks from the roots down to their Dependents; with `changed_only` there
    // is nothing to walk to, and only members are ever shown below a root.
    let shown_dependents = if opts.changed_only {
        BTreeMap::new()
    } else {
        affected::shown_dependents(&affected, &workspace.graph, |name| {
            workspace.is_member(name)
        })
    };
    let changed_non_members = changed
        .iter()
        .filter(|name| !workspace.is_member(name))
        .cloned()
        .collect();
    let members = affected
        .iter()
        .filter(|(_, reason)| !(opts.changed_only && **reason == Reason::Dependent))
        .filter_map(|(name, reason)| workspace.member(name).map(|pkg| (pkg, *reason)))
        .map(|(pkg, reason)| AffectedCrate {
            name: pkg.name.clone(),
            manifest_path: pkg.dir.join("Cargo.toml"),
            reason,
            changed_files: mapping.changed_files(&pkg.name),
            via: affected::via(&pkg.name, &affected, &workspace.graph),
        })
        .collect();
    Ok(Report {
        base,
        to,
        global_files,
        affected: members,
        changed_non_members,
        shown_dependents,
        file_outcomes: mapping.outcomes,
    })
}

/// When the workspace's lockfile is in the Change Set, the crates whose Locked
/// Dependencies changed between its two ends. `None` when the lockfile did not change or
/// the change cannot be narrowed (the lockfile is absent at either end, unreadable, or
/// differs outside its packages); it is then a Global File.
fn changed_locked_dependencies(
    repo: &Repo,
    change_set: &git::ChangeSet,
    workspace_in_repo: &Path,
    base: &str,
    to: &To,
) -> Result<Option<lockfile::LockedDependencies>> {
    let lockfile = workspace_in_repo.join(mapping::GlobalFiles::LOCKFILE);
    if !change_set.files.contains(&lockfile) {
        return Ok(None);
    }
    let old = repo.file_at(base, &lockfile)?;
    let new = match to {
        To::Commit(commit) => repo.file_at(commit, &lockfile)?,
        To::Worktree => std::fs::read_to_string(repo.root.join(&lockfile)).ok(),
    };
    Ok(old
        .zip(new)
        .and_then(|(old, new)| lockfile::changed_dependencies(&old, &new).ok()))
}
