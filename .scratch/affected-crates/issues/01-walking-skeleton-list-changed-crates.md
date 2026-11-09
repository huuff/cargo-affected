# 01: Walking skeleton: list Changed Crates

**What to build:** Running `cargo affected` inside a Cargo workspace prints, one per line and sorted by name, the workspace members whose own files appear in the Change Set. No propagation to Dependents yet. This ticket lands the whole pipeline end to end: resolve the Base, ask git for the Change Set, load the workspace with `cargo metadata`, map each file to the deepest crate whose directory contains it, print the result.

Change Set resolution as in the spec: `--base <ref>` (default `origin/main`, falling back to `main`) uses the fork point with HEAD; `--from <ref>` is exact and mutually exclusive with `--base`; `--to <ref>` defaults to the working tree including staged, unstaged and untracked-but-not-ignored files. Renames contribute both paths. The workspace may live below the git root; `--manifest-path` is honoured. Works both as `cargo affected` and as `cargo-affected`.

Also establishes the project shape: a library crate holding the logic, a thin binary, unit tests over synthetic inputs, and an integration test harness that copies a fixture workspace from the test fixtures directory to a temp dir, initialises a git repo there, commits a baseline, applies changes and runs the binary.

**Blocked by:** None (can start immediately).

**Status:** resolved

- [x] Editing a file inside crate `b` of a fixture workspace and running the binary with the default flags prints exactly `b`
- [x] Adding an untracked, non-ignored file inside crate `b` prints `b`; a gitignored new file prints nothing
- [x] With `--to HEAD`, uncommitted edits are not counted; with a commit on a branch off `main`, `--base main` reports the crates touched by that commit
- [x] `--from <tag> --to <tag>` reports the crates touched between two commits; passing both `--base` and `--from` is a usage error
- [x] A rename of a file from crate `a` into crate `b` prints both `a` and `b`
- [x] A fixture whose workspace root is a subdirectory of the git root maps files correctly
- [x] A nested crate (crate directory inside another crate's directory) claims its own files, the outer crate does not
- [x] An empty Change Set prints nothing and exits 0
- [x] `cargo affected` (via Cargo's subcommand dispatch) and direct invocation behave identically

## Comments

- 2026-11-09, agent: implemented. Library `cargo_affected` (`git`, `workspace`, `mapping` modules), clap surface in the binary, fixtures `tests/fixtures/{basic,nested}` and the harness in `tests/cli.rs`. Reviewed with `/code-review`; deferred to later tickets: workspace members outside the workspace root are silently unmapped (relevant to Extra Paths in 04), and a bad `--to` ref surfaces git's raw message (07).
