# 03: Global Files

**What to build:** A change to a Global File makes every workspace member an Affected Crate. Built-in Global Files are the workspace manifest, the lockfile, `rust-toolchain` and `rust-toolchain.toml`, and anything under the `.cargo` directory at the workspace root. Users extend the set with gitignore-style globs in `[workspace.metadata.affected].global`, matched relative to the workspace root. Global File matching runs before crate mapping, so a root package does not claim the workspace manifest as its own file. Each affected crate carries one reason with precedence `global` > `changed` > `dependent`; this ticket introduces the reason so later output formats can show it.

**Blocked by:** 01 (Walking skeleton).

**Status:** resolved

- [x] Editing `Cargo.lock` in a fixture prints every member
- [x] Editing a file under `.cargo/` prints every member
- [x] A fixture declaring `global = ["ci/**"]` prints every member when a file under `ci/` changes, and the built-in globals still apply
- [x] A fixture with a root package: editing only the workspace manifest yields reason `global` for every member, not `changed` for the root package
- [x] `--changed-only` still prints every member when a Global File changed
- [x] Unit tests cover the precedence rule when a crate is simultaneously global, changed and dependent

## Comments

- 2026-11-09, agent: implemented. `mapping::GlobalFiles` (built-ins + `[workspace.metadata.affected].global`, via `globset`) is consulted before crate ownership in `mapping::map_files`, which now returns the matched Global Files alongside the Changed Crates. New pure `affected` module holds `Reason` (`Global` > `Changed` > `Dependent`, encoded in the enum's `Ord`) and `affected_crates`, which layers global members, Changed Crates and the Dependent closure; the library API now returns `Vec<AffectedCrate { name, reason }>`. Fixture `tests/fixtures/globals` has a root package and `global = ["ci/**"]`. Notes: `cargo metadata` rewrites a hand-edited `Cargo.lock` back to its canonical form, so the lockfile integration test compares committed states with `--to HEAD`; `rust-toolchain{,.toml}` are covered by a unit test only, since a real file in a fixture would make a rustup-managed `cargo` try to install that toolchain.
