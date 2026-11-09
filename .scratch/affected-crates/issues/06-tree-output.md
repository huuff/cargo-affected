# 06: Tree output

**What to build:** `--format tree` explains why each crate is affected. Roots, sorted by name, are the Changed Crates, changed non-member path crates labelled `(non-member)`, and changed Global Files. Under a crate root hang its direct Dependents, recursively; a crate already printed elsewhere in the tree is shown once more with a `(*)` marker and not expanded, in the style of `cargo tree`. Under a Global File root every member is listed as a leaf. Unicode box-drawing characters by default, `--charset ascii` switches to ASCII connectors. No colour. `--changed-only` prints only the roots.

**Blocked by:** 02 (Propagate to Dependents), 03 (Global Files).

**Status:** ready-for-agent

- [ ] On the transitive fixture, editing `core` prints `core` as root with `mid` beneath it and `app` beneath `mid`
- [ ] In a diamond, the shared Dependent appears expanded once and marked `(*)` the second time
- [ ] Editing a non-member path dependency prints it as a root with the `(non-member)` label and its member Dependents beneath
- [ ] Editing `Cargo.lock` prints `Cargo.lock` as a root with every member as leaves and no further nesting
- [ ] `--charset ascii` output contains no non-ASCII bytes
- [ ] Rendering is unit-tested from a synthetic graph, independent of git and Cargo
