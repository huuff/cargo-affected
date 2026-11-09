# 09: Narrow lockfile changes to Locked Dependencies

**What to build:** A change to the workspace `Cargo.lock` no longer makes every member an Affected Crate. The lockfile at the Base is compared with the inspected one, and only the crates whose Locked Dependencies changed (third-party packages they use directly or through other third-party packages) become Changed Crates; their Dependents follow as usual. Rewritten entries of path packages, such as a member's version bump, claim nobody. The lockfile stays a Global File when the change cannot be narrowed, or when the user names it in `global`. See [ADR 0003](../../../docs/adr/0003-lockfile-narrowing.md).

**Blocked by:** 03 (Global Files).

**Status:** resolved

- [x] Bumping a third-party package in the lockfile prints the crates using it and their Dependents, not every member
- [x] Bumping a member's version and refreshing the lockfile prints that member and its Dependents only
- [x] A crate claimed by the lockfile has reason `changed` and `Cargo.lock` in `changed_files`; `global_files` is empty
- [x] `-v` prints `Cargo.lock -> <crate> (locked dependency: <name> <version>, ...)`, or `Cargo.lock -> no locked dependency changed`
- [x] A lockfile change outside the packages (format `version`) still prints every member
- [x] `global = ["Cargo.lock"]` makes any lockfile change print every member

## Comments

- 2026-11-09, agent: implemented. Motivated by a run on a 68-crate workspace where every commit that bumped a crate version reported all 68 crates. New pure module `src/lockfile.rs`: `changed_dependencies(old, new)` parses both lockfiles with `toml`, resolves dependency references (`name`, `name version`, `name version (source)`) to package ids so Cargo's respelling of references when a second version appears is not a change, and returns path package name -> `name version` labels. `Repo::file_at` reads the lockfile at a commit with `git show`; `lib.rs::changed_locked_dependencies` reads both ends before `cargo metadata` runs. `mapping::GlobalFiles` lost `Cargo.lock` from `BUILT_IN` and gained `narrow_lockfile`: un-narrowed, the lockfile is still a Global File, which is the fallback. Two new `Outcome`s, `LockedDependency(crate, labels)` and `NoLockedDependency`. Integration tests write third-party entries into the fixture lockfile by hand (`Fixture::lock_dependency`); no manifest names them, so `cargo metadata` needs no registry and drops them in memory only. The three tests that appended a comment to the lockfile to get "every member" now change the format version instead. Not done: the fallback is silent (`-v` just shows `global file`, not why narrowing failed), and JSON does not carry the labels.
