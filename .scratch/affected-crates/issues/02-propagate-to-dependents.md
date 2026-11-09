# 02: Propagate to Dependents and `--changed-only`

**What to build:** The list output grows from Changed Crates to Affected Crates: every workspace member that transitively depends on a Changed Crate is included. Every dependency written in any `Cargo.toml` is an edge regardless of kind (normal, dev, build), optionality or target cfg; feature state and host platform are ignored. Path-dependency crates that are not workspace members participate in propagation (a change in `vendor/foo` affects members depending on it) but are never printed. `--changed-only` restricts output to Changed Crates, suppressing crates that are affected only as Dependents.

**Blocked by:** 01 (Walking skeleton).

**Status:** resolved

- [x] In a fixture where `app` depends on `core`, editing `core` prints `app` and `core`
- [x] Transitive: `app` -> `mid` -> `core`, editing `core` prints all three
- [x] A dev-dependency, a build-dependency, an optional dependency behind a disabled feature and a target-specific dependency each propagate affectedness
- [x] Editing a non-member path dependency prints the members that depend on it but not the non-member itself
- [x] Diamond dependencies report each crate once
- [x] `--changed-only` on the transitive fixture prints only `core`
- [x] Unit tests cover the closure over a synthetic graph without touching git or Cargo

## Comments

- 2026-11-09, agent: implemented. New pure `graph` module (`DependencyGraph::affected` closure, unit-tested over synthetic edges); `Workspace` now loads every path package (members and non-members) and builds edges from the *declared* dependencies in `cargo metadata`, not the feature-resolved graph, so disabled optional and foreign-target dependencies count without `--all-features`. Non-members are mapped and propagate but are filtered from the output. `--changed-only` added. Fixture `tests/fixtures/graph` uses `workspace.exclude` for `vendor/foo` because Cargo auto-adopts in-root path dependencies as members.
- 2026-11-09, agent: reviewed with `/code-review`. Fixed in a follow-up commit: cycle unit test, a pass-through helper, doc drift. Recorded, not fixed: (1) the graph and the member filter are keyed by package *name*, so a non-member path crate sharing a name with a member (legal at different versions) would be conflated and printed; keying by `PackageId` would close it. (2) Dropping `no_deps` means `cargo metadata` now resolves registry dependencies, so an offline run without `Cargo.lock` fails where ticket 01 worked. (3) A path dependency outside the workspace root but inside the git root is never mapped (same deferral as ticket 01, see 04). (4) CONTEXT.md defines Changed Crate as a workspace member, but non-member path packages can now be Changed Crates that propagate without being printed; flag for `/domain-modeling`.
