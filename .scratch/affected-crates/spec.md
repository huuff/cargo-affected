# Spec: affected-crates

Status: ready-for-agent

`cargo affected` lists the workspace crates CI must run for, given what changed in git.
Vocabulary is defined in `CONTEXT.md`; the naming decision is `docs/adr/0001-crate-name-haf-cargo-affected.md`.

## Goal

Given a git repository containing a Cargo workspace, compute the set of Affected Crates
(Changed Crates plus all transitive Dependents) and print them as a list, a tree, or JSON,
so a CI pipeline can skip crates that are not affected.

## Change Set

The Change Set is `git diff <from> <to>` over file paths.

- `--base <ref>` (default: `origin/main`, falling back to `main`): `from` is `git merge-base <ref> HEAD`.
- `--from <ref>`: `from` is exactly `<ref>`. Mutually exclusive with `--base`.
- `--to <ref>`: default is the working tree, including staged, unstaged and untracked
  files that are not gitignored. `--to HEAD` restricts to committed work.
- Renamed files contribute both old and new paths. Deleted files contribute their path.
- A missing base ref, or no merge-base (shallow clone), is a hard error with a hint to fetch
  more history. Never silently fall back to "everything affected".
- Paths are normalised relative to the workspace root; the workspace may live below the git root.

## Mapping files to crates

Evaluated in order for each file in the Change Set:

1. **Global File**: matches a built-in global (workspace `Cargo.toml`, `Cargo.lock`,
   `rust-toolchain`, `rust-toolchain.toml`, `.cargo/**`) or a user glob from
   `[workspace.metadata.affected].global`. Every workspace member becomes Affected with
   reason `global`.
2. **Extra Path**: matches an `extra-paths` glob of some crate. That crate is Changed.
3. **Crate directory**: the file is under a crate's manifest directory. Deepest crate wins.
   The root package, when present, owns the whole repo like any other crate. If the file
   matches that crate's `ignore` globs, it is not counted.
4. Otherwise the file is unmapped and ignored. Listed on stderr under `-v`.

Globs are gitignore-style (`globset`) and anchored at their root, whatever their shape:
the workspace root for `global`, the crate directory for `extra-paths` and `ignore`. So
`README.md` matches only at that root and not `src/README.md` (write `**/README.md` for
any depth); `*` and `?` stay within one path component, and a leading `/` or `./` means
the same as no prefix. Decided in [ADR 0002](../../docs/adr/0002-glob-anchoring.md).
User globals extend the built-in set.

```toml
[workspace.metadata.affected]
global = ["ci/**", "deny.toml"]

[package.metadata.affected]
extra-paths = ["../proto/**"]
ignore = ["README.md", "docs/**"]
```

Crates present in the Change Set but absent from `cargo metadata` (deleted crates,
`workspace.exclude`d crates) are not reported; their files fall through to steps 3 and 4.

## Dependency graph

Nodes are all packages known to `cargo metadata` that are path dependencies reachable from
workspace members, plus the members themselves. Edges are every dependency written in a
`Cargo.toml`: normal, dev, build, optional, and target-specific all count. Feature state
and host platform are ignored.

Affectedness propagates from Changed Crates to all transitive Dependents. Non-member path
crates propagate but are never output in list or JSON. In the tree they appear as roots
labelled `(non-member)`.

Reason precedence per crate: `global` > `changed` > `dependent`.

## Output

`--format list|tree|json`, default `list`. All output sorted by package name. Only
workspace members are output.

- **list**: one package name per line. Empty Change Set prints nothing, exit 0.
- **tree**: roots are Changed Crates, non-member changed crates, and Global Files, sorted.
  Children of a crate are its direct Dependents, recursively; a node already printed is
  shown with `(*)` and not expanded (`cargo tree` style). Children of a Global File are
  all members as leaves. Unicode box drawing by default, `--charset ascii` alternative.
  No colour.
- **json**:
  ```json
  {
    "base": "<sha>",
    "to": "<sha>" | "worktree",
    "global_files": ["Cargo.lock"],
    "affected": [
      { "name": "api",
        "manifest_path": "/abs/crates/api/Cargo.toml",
        "reason": "changed" | "dependent" | "global",
        "changed_files": ["crates/api/src/lib.rs"],
        "via": ["core"] }
    ]
  }
  ```
  `via` lists the direct dependencies through which affectedness arrived (empty for
  `changed` and `global`).

`--changed-only` removes crates whose reason is `dependent` from every format.
`-v` prints the file-to-crate mapping and unmapped files to stderr. Without it stderr is
silent except for errors.

## CLI

Invoked as `cargo affected [flags]` (Cargo passes `affected` as the first argument) or
directly as `cargo-affected`. Supports `--manifest-path`.

## Implementation constraints

- Shell out to `git`; use `cargo_metadata` for the graph; `clap` for the CLI; `globset`.
- Library crate (`src/lib.rs`) with a thin binary (`src/main.rs`).
- Unit tests over synthetic graphs for mapping and closure. Integration tests copy fixture
  workspaces from `tests/fixtures/` to a temp dir, `git init` them, and run the binary.

## Out of scope for v1

- Precise `Cargo.lock` diffing (only crates depending on the bumped external dependency).
- Disabling built-in Global Files.
- Colourised output.
