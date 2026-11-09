# 05: JSON output

**What to build:** `--format json` prints a single JSON document to stdout: `base` (resolved commit of the from end), `to` (resolved commit or the string `worktree`), `global_files` (changed Global Files, workspace-relative), and `affected`, an array sorted by name with one object per Affected Crate holding `name`, `manifest_path` (absolute), `reason` (`global`, `changed` or `dependent`), `changed_files` (workspace-relative paths that mapped to this crate) and `via` (names of the direct dependencies through which affectedness arrived; empty for `changed` and `global`). Non-member crates never appear. `--changed-only` applies. The document is stable enough for a CI pipeline to build a job matrix with `jq`.

**Blocked by:** 02 (Propagate to Dependents), 03 (Global Files).

**Status:** resolved

- [x] On the transitive fixture, editing `core` yields `core` with reason `changed` and its file listed, `mid` with reason `dependent` and `via: ["core"]`, `app` with `via: ["mid"]`
- [x] A diamond where `app` depends on both `left` and `right`, both changed, yields `via: ["left", "right"]` sorted
- [x] Editing `Cargo.lock` yields `global_files: ["Cargo.lock"]` and every member with reason `global` and empty `via`
- [x] `to` is `worktree` by default and a commit hash with `--to HEAD`
- [x] Empty Change Set yields an empty `affected` array, exit 0
- [x] Output parses with a strict JSON parser and the array order is deterministic across runs

## Comments

- 2026-11-09, agent: implemented. The library now returns one analysis type, `report::Report` (`base`, `to`, `global_files`, `affected: Vec<AffectedCrate { name, manifest_path, reason, changed_files, via }>`), built by `cargo_affected::report(&Options)` in `src/lib.rs`, which replaces `affected_crates(&Options)`; `report::To` is `Commit(hash) | Worktree` and serialises as the hash or the string `worktree`. Every output format renders from it in `src/render.rs` (`render(&Report, Format) -> Result<String>`, `Format::{List, Json}`; ticket 06 adds `Tree` beside them), so `main.rs` only parses, reports, renders and prints. JSON comes from `serde` derives on the report types (`Reason` with `rename_all = "lowercase"`), pretty-printed with a trailing newline; every array is sorted (`BTreeMap`/`BTreeSet` throughout, no `HashMap` order leaks). Supporting pieces: `DependencyGraph::dependencies(name)` (the graph now keeps both directions), pure `affected::via(name, &affected, &graph)`, `Mapping::changed_files(crate)` (deduplicated, so a file that is both an Extra Path and inside the crate directory appears once), `Repo::rev_parse` made public to resolve `--to`, `Workspace::member(name)` replacing `is_member`. The CLI has `--format <list|json>` (clap `ValueEnum` in `cli.rs`, mapped onto `render::Format` so the library stays clap-free). Decision: `via` includes every affected direct dependency, member or not, so `uses-foo` affected through the excluded path crate `foo` reports `via: ["foo"]` even though `foo` itself never appears in `affected`; without this a Dependent could have an empty `via`, and the tree (ticket 06) shows `foo` as a `(non-member)` root anyway. `changed_files` is filled for a `global` crate too when its own files changed. The diamond checkbox uses the fixture's `top`/`left`/`right` (its `app` is the transitive chain). Note for ticket 06: `Report.affected` holds members only; the tree's non-member roots need either a `member` flag with a wider list or a separate field.
