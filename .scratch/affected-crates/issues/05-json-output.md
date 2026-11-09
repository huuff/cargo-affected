# 05: JSON output

**What to build:** `--format json` prints a single JSON document to stdout: `base` (resolved commit of the from end), `to` (resolved commit or the string `worktree`), `global_files` (changed Global Files, workspace-relative), and `affected`, an array sorted by name with one object per Affected Crate holding `name`, `manifest_path` (absolute), `reason` (`global`, `changed` or `dependent`), `changed_files` (workspace-relative paths that mapped to this crate) and `via` (names of the direct dependencies through which affectedness arrived; empty for `changed` and `global`). Non-member crates never appear. `--changed-only` applies. The document is stable enough for a CI pipeline to build a job matrix with `jq`.

**Blocked by:** 02 (Propagate to Dependents), 03 (Global Files).

**Status:** ready-for-agent

- [ ] On the transitive fixture, editing `core` yields `core` with reason `changed` and its file listed, `mid` with reason `dependent` and `via: ["core"]`, `app` with `via: ["mid"]`
- [ ] A diamond where `app` depends on both `left` and `right`, both changed, yields `via: ["left", "right"]` sorted
- [ ] Editing `Cargo.lock` yields `global_files: ["Cargo.lock"]` and every member with reason `global` and empty `via`
- [ ] `to` is `worktree` by default and a commit hash with `--to HEAD`
- [ ] Empty Change Set yields an empty `affected` array, exit 0
- [ ] Output parses with a strict JSON parser and the array order is deterministic across runs
