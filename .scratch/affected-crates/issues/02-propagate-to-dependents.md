# 02: Propagate to Dependents and `--changed-only`

**What to build:** The list output grows from Changed Crates to Affected Crates: every workspace member that transitively depends on a Changed Crate is included. Every dependency written in any `Cargo.toml` is an edge regardless of kind (normal, dev, build), optionality or target cfg; feature state and host platform are ignored. Path-dependency crates that are not workspace members participate in propagation (a change in `vendor/foo` affects members depending on it) but are never printed. `--changed-only` restricts output to Changed Crates, suppressing crates that are affected only as Dependents.

**Blocked by:** 01 (Walking skeleton).

**Status:** ready-for-agent

- [ ] In a fixture where `app` depends on `core`, editing `core` prints `app` and `core`
- [ ] Transitive: `app` -> `mid` -> `core`, editing `core` prints all three
- [ ] A dev-dependency, a build-dependency, an optional dependency behind a disabled feature and a target-specific dependency each propagate affectedness
- [ ] Editing a non-member path dependency prints the members that depend on it but not the non-member itself
- [ ] Diamond dependencies report each crate once
- [ ] `--changed-only` on the transitive fixture prints only `core`
- [ ] Unit tests cover the closure over a synthetic graph without touching git or Cargo
