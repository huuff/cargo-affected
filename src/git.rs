//! Talks to git: resolves the Base and computes the Change Set.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, anyhow, bail};

use crate::process::{stderr_of, stdout_of};

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
    /// Every file as a path relative to `dir`, itself relative to the repository root. A
    /// file outside `dir` climbs out of it with leading `..` components, so a workspace
    /// below the git root can still name every file of the Change Set.
    pub fn files_relative_to<'a>(&'a self, dir: &'a Path) -> impl Iterator<Item = PathBuf> + 'a {
        self.files.iter().map(move |file| relative_to(file, dir))
    }
}

/// `file` as a path from `dir`, both relative to the same root: the components after the
/// common prefix, preceded by one `..` for each component of `dir` not shared.
fn relative_to(file: &Path, dir: &Path) -> PathBuf {
    let mut file = file.components().peekable();
    let mut dir = dir.components().peekable();
    while file.peek().is_some() && file.peek() == dir.peek() {
        file.next();
        dir.next();
    }
    let mut path: PathBuf = dir.map(|_| "..").collect();
    path.extend(file);
    path
}

impl Repo {
    /// Finds the repository containing `dir`. Git's own "not a git repository" gets a
    /// message naming the directory; any other failure (permissions, `safe.directory`)
    /// is reported in git's words.
    pub fn discover(dir: &Path) -> Result<Self> {
        let args = ["rev-parse", "--show-toplevel"];
        let out = git_output(dir, &args)?;
        if !out.status.success() && stderr_of(&out).contains("not a git repository") {
            bail!(
                "{} is not inside a git repository; run `cargo affected` inside a checkout of the workspace",
                dir.display()
            );
        }
        let root = PathBuf::from(checked(&args, out)?.trim_end())
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

    /// The merge-base of `branch` and HEAD. Git exits 1 without a word when the two share
    /// no history, which in CI almost always means a shallow clone, so that case gets its
    /// own message; any other failure is reported in git's words.
    fn merge_base(&self, branch: &str) -> Result<String> {
        self.rev_parse(branch)?;
        let args = ["merge-base", branch, "HEAD"];
        let out = git_output(&self.root, &args)?;
        if out.status.code() == Some(1) && out.stderr.is_empty() {
            bail!(
                "no merge-base between `{branch}` and HEAD: the clone is shallow or the histories are unrelated; fetch more history (e.g. `git fetch --unshallow`, or `git fetch --deepen=<n>`) or pass another base with `--base <ref>`"
            );
        }
        Ok(checked(&args, out)?.trim_end().to_string())
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

/// Runs git and returns its stdout, failing with git's own words on a non-zero exit.
fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    checked(args, git_output(cwd, args)?)
}

/// Runs git and returns whatever it did, so the caller can read a non-zero exit itself.
/// Only failing to start git at all is an error here.
fn git_output(cwd: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("running git")
}

/// The stdout of a finished `git <args>`, or an error in git's own words.
fn checked(args: &[&str], out: Output) -> Result<String> {
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), stderr_of(&out));
    }
    stdout_of("git", out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_outside_the_directory_climbs_out_of_it_with_parent_components() {
        let rel = |file: &str, dir: &str| relative_to(Path::new(file), Path::new(dir));
        assert_eq!(
            rel("ws/crates/a/lib.rs", "ws"),
            Path::new("crates/a/lib.rs")
        );
        assert_eq!(rel("TOP.md", "ws"), Path::new("../TOP.md"));
        assert_eq!(rel("other/x.rs", "a/ws"), Path::new("../../other/x.rs"));
        assert_eq!(rel("a/x.rs", "a/ws"), Path::new("../x.rs"));
        assert_eq!(rel("wsx/y.rs", "ws"), Path::new("../wsx/y.rs"));
        assert_eq!(rel("README.md", ""), Path::new("README.md"));
    }

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
