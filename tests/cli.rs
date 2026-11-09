//! Integration tests: copy a fixture workspace to a temp dir, `git init` it, commit a
//! baseline, apply changes and run the binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    workspace: PathBuf,
}

impl Fixture {
    /// Copies `tests/fixtures/<name>` into `<workspace>` under a temp dir that becomes the
    /// git root, and commits everything as the baseline on `main`. An empty `workspace`
    /// puts the workspace at the git root.
    fn new(name: &str, workspace: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        copy_dir(&src, &root.join(workspace));
        let fixture = Self {
            _dir: dir,
            workspace: root.join(workspace),
            root,
        };
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.cargo_in_workspace(&["generate-lockfile", "--offline"]);
        fixture.commit("baseline");
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let out = git_command(&self.root).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn cargo_in_workspace(&self, args: &[&str]) {
        let out = Command::new(cargo())
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "cargo {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Runs `cargo-affected` directly, in the workspace directory.
    fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.workspace, args)
    }

    /// Runs `cargo-affected` directly, in `cwd`.
    fn run_in(&self, cwd: &Path, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-affected"));
        cmd.args(args).current_dir(cwd);
        git_env(&mut cmd);
        cmd.output().unwrap()
    }

    /// Runs `cargo affected`, letting Cargo dispatch to the binary found on `PATH`.
    fn run_via_cargo(&self, args: &[&str]) -> Output {
        let bin = Path::new(env!("CARGO_BIN_EXE_cargo-affected"));
        let mut path = bin.parent().unwrap().as_os_str().to_owned();
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        let mut cmd = Command::new(cargo());
        cmd.arg("affected")
            .args(args)
            .env("PATH", path)
            .current_dir(&self.workspace);
        git_env(&mut cmd);
        cmd.output().unwrap()
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

fn git_command(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd).args([
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.com",
    ]);
    git_env(&mut cmd);
    cmd
}

/// Isolates git from the developer's global configuration and hooks.
fn git_env(cmd: &mut Command) {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE");
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "exit {:?}, stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn lines(out: &Output) -> Vec<String> {
    stdout(out).lines().map(str::to_string).collect()
}

#[test]
fn editing_a_file_inside_crate_b_prints_b() {
    let fx = Fixture::new("basic", "");
    fx.write("crates/b/src/lib.rs", "pub fn b() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["b"]);
}

#[test]
fn untracked_file_counts_but_gitignored_file_does_not() {
    let fx = Fixture::new("basic", "");
    fx.write("crates/b/src/extra.rs", "pub fn extra() {}\n");
    assert_eq!(lines(&fx.run(&[])), ["b"]);

    let fx = Fixture::new("basic", "");
    fx.write("crates/b/build.log", "ignored by .gitignore\n");
    assert_eq!(lines(&fx.run(&[])), Vec::<String>::new());
}

#[test]
fn to_head_ignores_uncommitted_edits_and_base_main_reports_branch_commits() {
    let fx = Fixture::new("basic", "");
    fx.git(&["checkout", "-q", "-b", "feature"]);
    fx.write("crates/a/src/lib.rs", "pub fn a() { /* on branch */ }\n");
    fx.commit("touch a");
    fx.write("crates/b/src/lib.rs", "pub fn b() { /* uncommitted */ }\n");

    assert_eq!(lines(&fx.run(&["--to", "HEAD"])), ["a"]);
    assert_eq!(lines(&fx.run(&["--base", "main", "--to", "HEAD"])), ["a"]);
    assert_eq!(lines(&fx.run(&["--base", "main"])), ["a", "b"]);
}

#[test]
fn from_and_to_tags_report_changed_crates_between_them() {
    let fx = Fixture::new("basic", "");
    fx.git(&["tag", "v1"]);
    fx.write("crates/a/src/lib.rs", "pub fn a() { /* v2 */ }\n");
    fx.commit("touch a");
    fx.git(&["tag", "v2"]);
    fx.write("crates/b/src/lib.rs", "pub fn b() { /* after v2 */ }\n");
    fx.commit("touch b");

    assert_eq!(lines(&fx.run(&["--from", "v1", "--to", "v2"])), ["a"]);
}

#[test]
fn base_and_from_together_is_a_usage_error() {
    let fx = Fixture::new("basic", "");
    let out = fx.run(&["--base", "main", "--from", "v1"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--from"));
}

#[test]
fn renaming_a_file_from_crate_a_into_crate_b_prints_both() {
    let fx = Fixture::new("basic", "");
    fx.write("crates/a/src/util.rs", "pub fn util() { 1 + 1; }\n");
    fx.commit("add util");
    fx.git(&["mv", "crates/a/src/util.rs", "crates/b/src/util.rs"]);
    assert_eq!(lines(&fx.run(&[])), ["a", "b"]);
}

#[test]
fn workspace_below_the_git_root_maps_files_correctly() {
    let fx = Fixture::new("basic", "ws");
    fx.write("ws/crates/a/src/lib.rs", "pub fn a() { /* edited */ }\n");
    fx.write("README.md", "# edited above the workspace\n");
    assert_eq!(lines(&fx.run(&[])), ["a"]);
}

#[test]
fn manifest_path_selects_the_workspace_from_elsewhere() {
    let fx = Fixture::new("basic", "ws");
    fx.write("ws/crates/b/src/lib.rs", "pub fn b() { /* edited */ }\n");
    let manifest = fx.workspace.join("Cargo.toml");
    let out = fx.run_in(&fx.root, &["--manifest-path", manifest.to_str().unwrap()]);
    assert_eq!(lines(&out), ["b"]);
}

#[test]
fn nested_crate_claims_its_own_files() {
    let fx = Fixture::new("nested", "");
    fx.write(
        "outer/inner/src/lib.rs",
        "pub fn inner() { /* edited */ }\n",
    );
    assert_eq!(lines(&fx.run(&[])), ["inner"]);

    let fx = Fixture::new("nested", "");
    fx.write("outer/src/lib.rs", "pub fn outer() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["outer"]);
}

#[test]
fn empty_change_set_prints_nothing_and_exits_zero() {
    let fx = Fixture::new("basic", "");
    let out = fx.run(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"");
}

#[test]
fn cargo_subcommand_dispatch_and_direct_invocation_behave_identically() {
    let fx = Fixture::new("basic", "");
    fx.write("crates/b/src/lib.rs", "pub fn b() { /* edited */ }\n");
    let direct = fx.run(&["--base", "main"]);
    let via_cargo = fx.run_via_cargo(&["--base", "main"]);
    assert_eq!(stdout(&via_cargo), stdout(&direct));
    assert_eq!(lines(&via_cargo), ["b"]);
}

#[test]
fn missing_base_ref_is_an_error_naming_the_ref_with_a_fetch_hint() {
    let fx = Fixture::new("basic", "");
    let out = fx.run(&["--base", "origin/develop"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("origin/develop"), "{stderr}");
    assert!(stderr.contains("fetch"), "{stderr}");
    assert_eq!(out.stdout, b"");
}

#[test]
fn editing_a_crate_prints_it_and_its_direct_dependent() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/mid/src/lib.rs", "pub fn mid() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["app", "mid"]);
}

#[test]
fn affectedness_propagates_transitively_and_through_every_dependency_kind() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    // `mid` -> `core` and `app` -> `mid`; the `*-user` crates reach `core` through a
    // dev-, build-, disabled optional and `cfg(windows)` target dependency respectively.
    assert_eq!(
        lines(&fx.run(&[])),
        [
            "app",
            "build-user",
            "core",
            "dev-user",
            "mid",
            "opt-user",
            "target-user"
        ]
    );
}

#[test]
fn editing_a_non_member_path_dependency_prints_its_dependents_but_not_itself() {
    let fx = Fixture::new("graph", "");
    fx.write("vendor/foo/src/lib.rs", "pub fn foo() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["uses-foo"]);
}

#[test]
fn a_diamond_reports_each_crate_once() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/base/src/lib.rs", "pub fn base() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["base", "left", "right", "top"]);
}

#[test]
fn changed_only_leaves_out_crates_affected_as_dependents() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&["--changed-only"])), ["core"]);
}
