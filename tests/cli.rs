//! Integration tests: copy a fixture workspace to a temp dir, `git init` it, commit a
//! baseline, apply changes and run the binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;
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

#[test]
fn editing_the_lockfile_prints_every_member() {
    let fx = Fixture::new("basic", "");
    fx.git(&["checkout", "-q", "-b", "feature"]);
    let lockfile = fx.root.join("Cargo.lock");
    let edited = fs::read_to_string(&lockfile).unwrap() + "\n# touched\n";
    fx.write("Cargo.lock", &edited);
    fx.commit("touch lockfile");
    // `cargo metadata` may rewrite a hand-edited lockfile, so compare committed states.
    assert_eq!(
        lines(&fx.run(&["--base", "main", "--to", "HEAD"])),
        ["a", "b"]
    );
}

#[test]
fn editing_a_file_under_dot_cargo_prints_every_member() {
    let fx = Fixture::new("basic", "");
    fx.write(".cargo/config.toml", "[build]\n");
    assert_eq!(lines(&fx.run(&[])), ["a", "b"]);
}

#[test]
fn a_user_global_glob_prints_every_member_and_built_ins_still_apply() {
    let fx = Fixture::new("globals", "");
    fx.write("ci/test.sh", "#!/bin/sh\necho edited\n");
    assert_eq!(lines(&fx.run(&[])), ["a", "root"]);

    let fx = Fixture::new("globals", "");
    fx.write(".cargo/config.toml", "[build]\n");
    assert_eq!(lines(&fx.run(&[])), ["a", "root"]);
}

#[test]
fn editing_only_the_workspace_manifest_is_global_for_every_member_including_the_root_package() {
    use cargo_affected::{Options, Reason, report};

    let fx = Fixture::new("globals", "");
    let manifest = fx.root.join("Cargo.toml");
    let edited = fs::read_to_string(&manifest).unwrap() + "\n# touched\n";
    fx.write("Cargo.toml", &edited);

    let opts = Options {
        base: Some("main".into()),
        manifest_path: Some(manifest),
        ..Options::default()
    };
    let got: Vec<(String, Reason)> = report(&opts)
        .unwrap()
        .affected
        .into_iter()
        .map(|c| (c.name, c.reason))
        .collect();
    assert_eq!(
        got,
        [
            ("a".to_string(), Reason::Global),
            ("root".to_string(), Reason::Global)
        ]
    );
}

#[test]
fn changed_only_still_prints_every_member_when_a_global_file_changed() {
    let fx = Fixture::new("basic", "");
    fx.write(".cargo/config.toml", "[build]\n");
    assert_eq!(lines(&fx.run(&["--changed-only"])), ["a", "b"]);
}

#[test]
fn editing_an_extra_path_of_codegen_prints_codegen() {
    let fx = Fixture::new("extra-paths", "");
    fx.write("proto/service.proto", "syntax = \"proto3\";\n// edited\n");
    assert_eq!(lines(&fx.run(&[])), ["codegen"]);
}

#[test]
fn editing_an_ignored_file_of_lib_prints_nothing_while_its_source_prints_lib() {
    let fx = Fixture::new("extra-paths", "");
    fx.write("lib/README.md", "# lib\n\nEdited.\n");
    fx.write("lib/docs/guide.md", "# Guide\n\nEdited.\n");
    assert_eq!(lines(&fx.run(&[])), Vec::<String>::new());

    let fx = Fixture::new("extra-paths", "");
    fx.write("lib/src/lib.rs", "pub fn lib() { /* edited */ }\n");
    assert_eq!(lines(&fx.run(&[])), ["lib"]);
}

#[test]
fn an_extra_path_inside_another_crate_prints_both_crates() {
    let fx = Fixture::new("extra-paths", "");
    fx.write("schema/src/types.rs", "pub struct Greeting; // edited\n");
    assert_eq!(lines(&fx.run(&[])), ["codegen", "schema"]);
}

#[test]
fn a_crate_without_a_metadata_table_is_mapped_by_its_directory_as_before() {
    let fx = Fixture::new("extra-paths", "");
    fx.write("schema/src/lib.rs", "pub mod types; // edited\n");
    assert_eq!(lines(&fx.run(&[])), ["schema"]);
}

#[test]
fn malformed_package_metadata_is_an_error_naming_the_crate_and_the_setting() {
    let fx = Fixture::new("malformed-metadata", "");
    fx.write("broken/src/lib.rs", "pub fn broken() { /* edited */ }\n");
    let out = fx.run(&[]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("`broken`"), "{stderr}");
    assert!(stderr.contains("extra-paths"), "{stderr}");
    assert!(stderr.contains("array of strings"), "{stderr}");
    assert_eq!(out.stdout, b"");
}

/// Parses the JSON document a run printed, failing on anything a strict parser rejects.
fn parse_json(out: &Output) -> serde_json::Value {
    let text = stdout(out);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{text}"))
}

/// The entry of the `affected` array for `name`.
fn affected<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["affected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no `{name}` in {doc:#}"))
}

fn affected_names(doc: &serde_json::Value) -> Vec<&str> {
    doc["affected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect()
}

#[test]
fn json_lists_the_changed_crate_with_its_file_and_each_dependent_with_its_via() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    let doc = parse_json(&fx.run(&["--format", "json"]));

    let core = affected(&doc, "core");
    assert_eq!(core["reason"], "changed");
    assert_eq!(core["changed_files"], json!(["crates/core/src/lib.rs"]));
    assert_eq!(core["via"], json!([]));
    assert_eq!(
        core["manifest_path"],
        json!(fx.root.join("crates/core/Cargo.toml"))
    );

    let mid = affected(&doc, "mid");
    assert_eq!(mid["reason"], "dependent");
    assert_eq!(mid["changed_files"], json!([]));
    assert_eq!(mid["via"], json!(["core"]));

    let app = affected(&doc, "app");
    assert_eq!(app["reason"], "dependent");
    assert_eq!(app["via"], json!(["mid"]));
    assert_eq!(doc["global_files"], json!([]));
}

#[test]
fn json_via_of_a_diamond_lists_both_changed_sides_sorted() {
    let fx = Fixture::new("graph", "");
    fx.write(
        "crates/right/src/lib.rs",
        "pub fn right() { /* edited */ }\n",
    );
    fx.write("crates/left/src/lib.rs", "pub fn left() { /* edited */ }\n");
    let doc = parse_json(&fx.run(&["--format", "json"]));

    assert_eq!(affected_names(&doc), ["left", "right", "top"]);
    assert_eq!(affected(&doc, "left")["reason"], "changed");
    assert_eq!(affected(&doc, "right")["reason"], "changed");
    let top = affected(&doc, "top");
    assert_eq!(top["reason"], "dependent");
    assert_eq!(top["via"], json!(["left", "right"]));
}

#[test]
fn json_lists_the_lockfile_as_a_global_file_and_every_member_as_global_without_via() {
    let fx = Fixture::new("basic", "");
    fx.git(&["checkout", "-q", "-b", "feature"]);
    let lockfile = fx.root.join("Cargo.lock");
    let edited = fs::read_to_string(&lockfile).unwrap() + "\n# touched\n";
    fx.write("Cargo.lock", &edited);
    fx.commit("touch lockfile");
    let doc = parse_json(&fx.run(&["--format", "json", "--base", "main", "--to", "HEAD"]));

    assert_eq!(doc["global_files"], json!(["Cargo.lock"]));
    assert_eq!(affected_names(&doc), ["a", "b"]);
    for name in ["a", "b"] {
        let c = affected(&doc, name);
        assert_eq!(c["reason"], "global", "{name}");
        assert_eq!(c["via"], json!([]), "{name}");
        assert_eq!(c["changed_files"], json!([]), "{name}");
    }
}

#[test]
fn json_base_is_the_merge_base_and_to_is_worktree_by_default_or_a_commit_with_to_head() {
    let fx = Fixture::new("basic", "");
    let fork = fx.git(&["rev-parse", "HEAD"]);
    fx.git(&["checkout", "-q", "-b", "feature"]);
    fx.write("crates/b/src/lib.rs", "pub fn b() { /* edited */ }\n");
    fx.commit("edit b");
    let head = fx.git(&["rev-parse", "HEAD"]);
    // `main` moves on after the fork, so the Base is the fork, not `main`'s tip.
    fx.git(&["checkout", "-q", "main"]);
    fx.write(
        "crates/a/src/lib.rs",
        "pub fn a() { /* edited on main */ }\n",
    );
    fx.commit("edit a on main");
    fx.git(&["checkout", "-q", "feature"]);
    assert_ne!(fx.git(&["rev-parse", "main"]), fork);

    let doc = parse_json(&fx.run(&["--format", "json", "--base", "main"]));
    assert_eq!(doc["base"], fork);
    assert_eq!(doc["to"], "worktree");

    let doc = parse_json(&fx.run(&["--format", "json", "--base", "main", "--to", "HEAD"]));
    assert_eq!(doc["base"], fork);
    assert_eq!(doc["to"], head);
    assert_eq!(affected_names(&doc), ["b"]);
}

#[test]
fn json_of_an_empty_change_set_has_an_empty_affected_array_and_exits_zero() {
    let fx = Fixture::new("basic", "");
    let out = fx.run(&["--format", "json"]);
    assert_eq!(out.status.code(), Some(0));
    let doc = parse_json(&out);
    assert_eq!(doc["affected"], json!([]));
    assert_eq!(doc["global_files"], json!([]));
}

#[test]
fn json_is_identical_across_runs_with_the_affected_array_sorted_by_name() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    let first = stdout(&fx.run(&["--format", "json"]));
    let second = stdout(&fx.run(&["--format", "json"]));
    assert_eq!(first, second);

    let doc: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(
        affected_names(&doc),
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
    assert_eq!(doc.as_object().unwrap().len(), 4, "{doc:#}");
    assert_eq!(
        affected(&doc, "core")
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["changed_files", "manifest_path", "name", "reason", "via"]
    );
}

#[test]
fn json_changed_only_leaves_out_dependents() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    let doc = parse_json(&fx.run(&["--format", "json", "--changed-only"]));
    assert_eq!(affected_names(&doc), ["core"]);
}

#[test]
fn json_never_lists_a_non_member_but_names_it_in_via() {
    let fx = Fixture::new("graph", "");
    fx.write("vendor/foo/src/lib.rs", "pub fn foo() { /* edited */ }\n");
    let doc = parse_json(&fx.run(&["--format", "json"]));
    assert_eq!(affected_names(&doc), ["uses-foo"]);
    let c = affected(&doc, "uses-foo");
    assert_eq!(c["reason"], "dependent");
    assert_eq!(c["via"], json!(["foo"]));
    assert_eq!(c["changed_files"], json!([]));
}

#[test]
fn tree_prints_the_changed_crate_as_root_with_each_dependent_beneath_the_crate_it_uses() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree"])),
        "core\n\
         ├── build-user\n\
         ├── dev-user\n\
         ├── mid\n\
         │   └── app\n\
         ├── opt-user\n\
         └── target-user\n"
    );
}

#[test]
fn tree_expands_the_shared_dependent_of_a_diamond_once_and_marks_it_the_second_time() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/base/src/lib.rs", "pub fn base() { /* edited */ }\n");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree"])),
        "base\n\
         ├── left\n\
         │   └── top\n\
         └── right\n\
         \x20   └── top (*)\n"
    );
}

#[test]
fn tree_shows_an_edited_non_member_path_dependency_as_a_labelled_root() {
    let fx = Fixture::new("graph", "");
    fx.write("vendor/foo/src/lib.rs", "pub fn foo() { /* edited */ }\n");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree"])),
        "foo (non-member)\n\
         └── uses-foo\n"
    );
}

#[test]
fn tree_lists_every_member_as_a_leaf_under_the_edited_lockfile() {
    let fx = Fixture::new("basic", "");
    fx.git(&["checkout", "-q", "-b", "feature"]);
    let lockfile = fx.root.join("Cargo.lock");
    let edited = fs::read_to_string(&lockfile).unwrap() + "\n# touched\n";
    fx.write("Cargo.lock", &edited);
    fx.commit("touch lockfile");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree", "--base", "main", "--to", "HEAD"])),
        "Cargo.lock\n\
         ├── a\n\
         └── b\n"
    );
}

#[test]
fn tree_with_ascii_charset_contains_no_non_ascii_bytes() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/base/src/lib.rs", "pub fn base() { /* edited */ }\n");
    let out = fx.run(&["--format", "tree", "--charset", "ascii"]);
    let text = stdout(&out);
    assert!(out.stdout.is_ascii(), "{text}");
    assert_eq!(
        text,
        "base\n\
         |-- left\n\
         |   `-- top\n\
         `-- right\n\
         \x20   `-- top (*)\n"
    );
}

#[test]
fn tree_with_changed_only_prints_only_the_roots() {
    let fx = Fixture::new("graph", "");
    fx.write("crates/core/src/lib.rs", "pub fn core() { /* edited */ }\n");
    fx.write("vendor/foo/src/lib.rs", "pub fn foo() { /* edited */ }\n");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree", "--changed-only"])),
        "core\nfoo (non-member)\n"
    );
}

#[test]
fn tree_with_changed_only_keeps_every_member_under_a_global_file() {
    // The members are affected by the Global File itself, not as Dependents, so
    // `--changed-only` has nothing to remove: the leaves are the roots' whole story.
    let fx = Fixture::new("basic", "");
    fx.write(".cargo/config.toml", "[build]\n");
    assert_eq!(
        stdout(&fx.run(&["--format", "tree", "--changed-only"])),
        ".cargo/config.toml\n\
         ├── a\n\
         └── b\n"
    );
}
