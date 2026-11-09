//! Talks to git: resolves the Base and computes the Change Set.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

/// Branches tried, in order, when no `--base` is given.
pub const DEFAULT_BASE_BRANCHES: [&str; 2] = ["origin/main", "main"];

/// The git repository containing the workspace.
pub struct Repo {
    /// Canonical path of the working tree root.
    pub root: PathBuf,
}

/// How the `from` end of the Change Set is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// Merge-base of the given branch and HEAD; `None` tries [`DEFAULT_BASE_BRANCHES`].
    MergeBase(Option<String>),
    /// Exactly this ref.
    Exact(String),
}

/// The files git reports as different between the Base and the inspected state,
/// as paths relative to the repository root. Renames contribute both paths.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    pub files: Vec<PathBuf>,
}

impl ChangeSet {
    /// The files under `dir` (relative to the repository root), as paths relative to `dir`.
    pub fn files_under<'a>(&'a self, dir: &'a Path) -> impl Iterator<Item = &'a Path> {
        self.files
            .iter()
            .filter_map(move |f| f.strip_prefix(dir).ok())
    }
}

impl Repo {
    /// Finds the repository containing `dir`.
    pub fn discover(dir: &Path) -> Result<Self> {
        let out = run_git(dir, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("{} is not inside a git repository", dir.display()))?;
        let root = PathBuf::from(out.trim_end())
            .canonicalize()
            .context("canonicalising the git root")?;
        Ok(Self { root })
    }

    /// Resolves the Base to a commit hash.
    pub fn resolve_base(&self, base: &Base) -> Result<String> {
        match base {
            Base::Exact(rev) => self.rev_parse(rev),
            Base::MergeBase(Some(branch)) => self.merge_base(branch),
            Base::MergeBase(None) => {
                let branch = DEFAULT_BASE_BRANCHES
                    .into_iter()
                    .find(|b| self.rev_parse(b).is_ok())
                    .ok_or_else(|| {
                        let tried = DEFAULT_BASE_BRANCHES
                            .map(|b| format!("`{b}`"))
                            .join(" nor ");
                        anyhow!("neither {tried} exists; pass the base branch with `--base <ref>`")
                    })?;
                self.merge_base(branch)
            }
        }
    }

    /// Change Set between `from` and `to`; `None` inspects the working tree, including
    /// staged, unstaged and untracked-but-not-ignored files.
    pub fn change_set(&self, from: &str, to: Option<&str>) -> Result<ChangeSet> {
        let mut args = vec!["diff", "--name-status", "-z", "-M", from];
        if let Some(to) = to {
            args.push(to);
        }
        let name_status = self.git(&args)?;
        let mut files = parse_name_status(&name_status);
        if to.is_none() {
            let untracked = self.git(&["ls-files", "--others", "--exclude-standard", "-z"])?;
            files.extend(
                untracked
                    .split('\0')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from),
            );
        }
        files.sort();
        files.dedup();
        Ok(ChangeSet { files })
    }

    /// Resolves `rev` to a commit hash.
    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        let out = self
            .git(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ])
            .map_err(|_| {
                anyhow!(
                    "ref `{rev}` does not exist; fetch it first (e.g. `git fetch origin {rev}`)"
                )
            })?;
        Ok(out.trim_end().to_string())
    }

    fn merge_base(&self, branch: &str) -> Result<String> {
        self.rev_parse(branch)?;
        let out = self.git(&["merge-base", branch, "HEAD"]).with_context(|| {
            format!(
                "no merge-base between `{branch}` and HEAD; fetch more history (e.g. `git fetch --unshallow`)"
            )
        })?;
        Ok(out.trim_end().to_string())
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        run_git(&self.root, args)
    }
}

/// Parses `git diff --name-status -z` output into paths; renames and copies yield both.
fn parse_name_status(out: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    while let Some(status) = fields.next() {
        let Some(path) = fields.next() else { break };
        files.push(PathBuf::from(path));
        if (status.starts_with('R') || status.starts_with('C'))
            && let Some(new) = fields.next()
        {
            files.push(PathBuf::from(new));
        }
    }
    files
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).context("git output is not UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_contribute_both_paths_and_deletions_their_path() {
        let out = "M\0a/src/lib.rs\0R100\0a/src/util.rs\0b/src/util.rs\0D\0b/old.rs\0";
        let got = parse_name_status(out);
        let want: Vec<PathBuf> = ["a/src/lib.rs", "a/src/util.rs", "b/src/util.rs", "b/old.rs"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(got, want);
    }
}
