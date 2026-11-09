# haf-cargo-affected

`cargo affected` lists the workspace crates CI needs to run for, given what changed in
git. It diffs two git states, maps every changed file to the crate that owns it, follows
the dependency graph to everything that depends on those crates, and prints the result as
a list, a tree or JSON.

```console
$ echo '// edit' >> crates/base/src/lib.rs    # in tests/fixtures/graph
$ cargo affected
base
left
right
top
```

The package is named `haf-cargo-affected` because `cargo-affected` on crates.io is an
unrelated tool; see [Name](#name). The binary is still `cargo-affected`, so Cargo finds it
as `cargo affected`.

## Install

From a checkout:

```sh
cargo install --path . --locked
```

With Nix (the flake exposes the package as `packages.default` and an overlay):

```sh
nix build            # ./result/bin/cargo-affected
nix run . -- --help
nix profile install .
```

Once published on crates.io: `cargo install --locked haf-cargo-affected`.

The binary shells out to `git` and `cargo`, so both must be on `PATH`.

## Mental model

The vocabulary below is defined in [`CONTEXT.md`](CONTEXT.md); this README uses it
exactly.

- The **Change Set** is the set of files git reports as different between two states of
  the repository: `git diff <base> <to>` over paths. Renamed files contribute both paths,
  deleted files their path.
- The **Base** is the git state the Change Set is computed against. By default it is the
  merge-base of `origin/main` (or `main`) and `HEAD`, so a feature branch is compared
  against the point where it forked, not against the tip of `main`. `--base` picks another
  branch, `--from` an exact ref.
- The `to` end defaults to the working tree: staged, unstaged and untracked files that are
  not gitignored. `--to HEAD` restricts it to committed work.
- A **Changed Crate** is a workspace member whose own files appear in the Change Set.
- A **Dependent** is a crate that depends on another, directly or transitively.
- An **Affected Crate** is a Changed Crate, any workspace member that is a Dependent of
  one, or every workspace member when a **Global File** changed.
- A **Global File** is a file outside any crate whose change makes every member an
  Affected Crate: the workspace `Cargo.toml`, `Cargo.lock`, `rust-toolchain`,
  `rust-toolchain.toml`, anything under `.cargo/`, plus the globs you add in
  [`global`](#configuration).
- An **Extra Path** is a path outside a crate's directory that the crate declares as its
  own (a `proto/` directory its build script reads, say), so changes there make it a
  Changed Crate.

### How a file maps to a crate

Each file of the Change Set is tried against these rules in order; the first that applies
wins:

1. **Global File**: a built-in global or a `global` glob matches. Every member becomes an
   Affected Crate with reason `global`.
2. **Extra Path**: an `extra-paths` glob of some crate matches. That crate is Changed.
3. **Crate directory**: the file lies under a crate's manifest directory; the deepest crate
   wins, and a root package owns the whole workspace like any other crate. If the file
   matches that crate's `ignore` globs it does not make the crate Changed.
4. Otherwise the file is unmapped and ignored. `-v` lists it.

Rules 2 and 3 are not exclusive: a file inside crate `schema` that is also an Extra Path of
`codegen` makes both crates Changed.

### How affectedness spreads

The dependency graph holds every workspace member plus the path dependencies reachable
from them. Every dependency written in a `Cargo.toml` is an edge: normal, dev, build,
optional and target-specific dependencies all count, and feature state and host platform
are ignored. Editing `core` in [`tests/fixtures/graph`](tests/fixtures/graph) therefore
affects `build-user` (build-dependency), `dev-user` (dev-dependency), `opt-user` (optional)
and `target-user` (`cfg(windows)`) as well as `mid` and, through `mid`, `app`.

Each Affected Crate carries one reason, with precedence `global` > `changed` >
`dependent`. Only workspace members are ever listed; a non-member path crate (one under
`workspace.exclude`, say) propagates affectedness to its Dependents but is not itself
reported.

## Flags

Invoke as `cargo affected [OPTIONS]`, or directly as `cargo-affected [OPTIONS]`; the two
behave identically.

| Flag                                 | Default                                 | Meaning                                                                                                                                               |
| ------------------------------------ | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--base <REF>`                       | `origin/main`, falling back to `main`   | Branch to compute the Base from: the Base is the merge-base of `REF` and `HEAD`. Conflicts with `--from`.                                             |
| `--from <REF>`                       | unset                                   | Use exactly `REF` as the Base instead of a merge-base.                                                                                                |
| `--to <REF>`                         | the working tree                        | Ref to compare against. Without it the Change Set includes staged, unstaged and untracked files that are not gitignored; `--to HEAD` means committed work only. |
| `--manifest-path <PATH>`             | the `Cargo.toml` found upward from `.`  | Path to the workspace `Cargo.toml`. The workspace may sit below the git root.                                                                         |
| `--changed-only`                     | off                                     | Leave out the crates whose reason is `dependent`. Crates with reason `global` stay in.                                                                |
| `--format <list\|json\|tree>`        | `list`                                  | Output format, see [Output formats](#output-formats).                                                                                                 |
| `--charset <unicode\|ascii>`         | `unicode`                               | Connectors of `--format tree`: box drawing, or plain ASCII for logs without UTF-8. Accepted and ignored by the other formats.                        |
| `-v`, `--verbose`                    | off                                     | Explain on stderr where every file of the Change Set landed, see [Diagnostics](#diagnostics). Without it a successful run writes nothing to stderr.  |
| `-h`, `--help`                       |                                         | Print help (`-h` for the one-line summary of each flag).                                                                                              |
| `-V`, `--version`                    |                                         | Print the version.                                                                                                                                    |

`--base` and `--from` together is a usage error (exit 2).

## Output formats

The examples are real runs on [`tests/fixtures/graph`](tests/fixtures/graph), whose
members include the chain `core` -> `mid` -> `app`, the diamond `base` -> `left`/`right`
-> `top`, and `uses-foo`, which depends on the non-member path crate `vendor/foo`. All
output is sorted by crate name.

### `list` (default)

One crate name per line. An empty Change Set prints nothing and exits 0, which lets a
script treat "no output" as "nothing to run".

```console
$ echo '// edit' >> crates/core/src/lib.rs
$ cargo affected
app
build-user
core
dev-user
mid
opt-user
target-user
$ cargo affected --changed-only
core
```

### `tree`

Why each crate is affected, `cargo tree` style. Roots are the crates whose own files
changed, non-member crates whose files changed (labelled `(non-member)`) and Global Files,
sorted together by name. Under a crate hang its direct Dependents, recursively; a crate
already expanded is shown as `name (*)` and not expanded again. Under a Global File every
Affected Crate is listed as a leaf.

```console
$ cargo affected --format tree
core
├── build-user
├── dev-user
├── mid
│   └── app
├── opt-user
└── target-user
```

The same edit with `--charset ascii`:

```text
core
|-- build-user
|-- dev-user
|-- mid
|   `-- app
|-- opt-user
`-- target-user
```

The diamond, after editing `crates/base/src/lib.rs`:

```text
base
├── left
│   └── top
└── right
    └── top (*)
```

A non-member path crate as root, after editing `vendor/foo/src/lib.rs`:

```text
foo (non-member)
└── uses-foo
```

And a Global File next to a Changed Crate, comparing a branch that edited both
`Cargo.lock` and `crates/core/src/lib.rs` (`--to HEAD`, members abbreviated):

```text
Cargo.lock
├── app
├── base
├── build-user
├── core
...
└── uses-foo
core
├── build-user
├── dev-user
├── mid
│   └── app
├── opt-user
└── target-user
```

`--changed-only` prints only the roots, keeping every member under a Global File.

### `json`

One pretty-printed document: the Base and `to` commits (`to` is the string `"worktree"`
when the working tree is inspected), the Global Files in the Change Set, and one entry per
Affected Crate with its absolute manifest path, its reason, the files that made it Changed
(relative to the workspace root) and `via`, the direct dependencies through which
affectedness arrived. `via` is empty for `changed` and `global` crates. After editing
`vendor/foo/src/lib.rs` (manifest paths abbreviated):

```console
$ cargo affected --format json
{
  "base": "f4e56dbd012d6b2f9c932b8a91a4856f4dbd855e",
  "to": "worktree",
  "global_files": [],
  "affected": [
    {
      "name": "uses-foo",
      "manifest_path": "/work/graph/crates/uses-foo/Cargo.toml",
      "reason": "dependent",
      "changed_files": [],
      "via": [
        "foo"
      ]
    }
  ]
}
```

For the diamond, `top` reports `"via": ["left", "right"]`; for the `core` edit above,
`core` is `"reason": "changed"` with `"changed_files": ["crates/core/src/lib.rs"]` and
`app` is `"reason": "dependent"` with `"via": ["mid"]`. When `Cargo.lock` changed,
`global_files` is `["Cargo.lock"]` and every member has `"reason": "global"`. An empty
Change Set yields `"affected": []` and exits 0. The document is identical across runs for
the same inputs, so it is safe to diff or cache.

## Configuration

Three settings, all lists of globs, live in Cargo metadata tables. Each is exercised by a
fixture in this repository.

### Workspace: Global Files

| Key                                   | Type               | Root for globs | Meaning                                                                                        | Fixture                                                                               |
| ------------------------------------- | ------------------ | -------------- | ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `[workspace.metadata.affected].global` | array of strings   | workspace root | Extra Global Files, extending the built-in set (`Cargo.toml`, `Cargo.lock`, `rust-toolchain`, `rust-toolchain.toml`, `.cargo/**`). A match makes every member affected with reason `global`. | [`tests/fixtures/globals`](tests/fixtures/globals/Cargo.toml) (`global = ["ci/**"]`) |

```toml
[workspace.metadata.affected]
global = ["ci/**", "deny.toml"]
```

The built-in set cannot be disabled.

### Crate: Extra Paths and ignore

| Key                                        | Type             | Root for globs      | Meaning                                                                                                                                       | Fixture                                                                                               |
| ------------------------------------------ | ---------------- | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `[package.metadata.affected].extra-paths`  | array of strings | the crate directory | Extra Paths: files outside the crate that make it a Changed Crate. May start with `../`; a glob that climbs above the workspace root is an error. | [`tests/fixtures/extra-paths`](tests/fixtures/extra-paths/codegen/Cargo.toml) (`codegen`)            |
| `[package.metadata.affected].ignore`       | array of strings | the crate directory | Files inside the crate directory that do not make it a Changed Crate. Does not stop another crate's Extra Path from claiming the file.        | [`tests/fixtures/extra-paths`](tests/fixtures/extra-paths/lib/Cargo.toml) (`lib`)                    |

```toml
# codegen/Cargo.toml: the build script reads the sibling `proto` directory and one file of `schema`.
[package.metadata.affected]
extra-paths = ["../proto/**", "../schema/src/types.rs"]

# lib/Cargo.toml: documentation changes do not need lib's tests.
[package.metadata.affected]
ignore = ["README.md", "docs/**"]
```

A wrong type is an error naming the crate and the setting
(`` `[package.metadata.affected].extra-paths` of crate `broken` must be an array of strings ``);
a misspelled key such as `extra_paths` is silently ignored, as Cargo metadata always is.

### Glob rules

One rule for all three settings, decided in
[ADR 0002](docs/adr/0002-glob-anchoring.md): **every glob is anchored at its root**, the
workspace root for `global` and the crate's own directory for `extra-paths` and `ignore`,
whatever the glob's shape. `*` and `?` stay within one path component, `**` spans
directories, and a leading `/` or `./` is accepted and means the same as no prefix.

So `*.toml` matches `deny.toml` but not `ci/deny.toml`; write `**/*.toml` to match at any
depth, and `README.md` in a crate's `ignore` leaves `src/README.md` alone. A `.gitignore`
line like `*.md` needs `**/` in front to keep its meaning here. A pattern never matches
more than it literally spells out, so a short `ignore` glob cannot silently hide changes
deeper in the crate.

## Diagnostics

`-v` prints one line per file of the Change Set to stderr, before the result on stdout:

```text
<file> -> <outcome>
```

where `<file>` is relative to the workspace root and `<outcome>` is one of
`<crate> (crate directory)`, `<crate> (extra path)`, `<crate> (ignored)`, `global file` or
`unmapped`. A file claimed by two crates appears once per crate. From
[`tests/fixtures/extra-paths`](tests/fixtures/extra-paths) after editing five files:

```console
$ cargo affected -v
TOP.md -> unmapped
lib/README.md -> lib (ignored)
lib/docs/guide.md -> lib (ignored)
proto/service.proto -> codegen (extra path)
schema/src/types.rs -> codegen (extra path)
schema/src/types.rs -> schema (crate directory)
codegen
schema
```

Without `-v`, stderr carries only errors.

## Errors and exit codes

| Exit | When                                                                                   |
| ---- | -------------------------------------------------------------------------------------- |
| 0    | Success, including an empty Change Set (empty list, empty tree, `"affected": []`).     |
| 1    | Any failure below. Nothing is printed on stdout.                                       |
| 2    | Usage error, such as `--base` together with `--from` or an unknown `--format`.         |

The tool never falls back to "everything is affected": when it cannot compute the Change
Set it fails, so a misconfigured pipeline fails loudly instead of quietly running or
skipping every crate.

- A `--base` or `--from` ref that does not exist:
  ``error: ref `does-not-exist` does not exist; fetch it first (e.g. `git fetch origin does-not-exist`)``
- No merge-base, which in CI almost always means a shallow clone:
  ``error: no merge-base between `origin/main` and HEAD: the clone is shallow or the histories are unrelated; fetch more history (e.g. `git fetch --unshallow`, or `git fetch --deepen=<n>`) or pass another base with `--base <ref>` ``
- No `--base` and neither default branch exists:
  ``error: neither `origin/main` nor `main` exists; pass the base branch with `--base <ref>` ``
- Outside a git repository:
  `error: <dir> is not inside a git repository; run `cargo affected` inside a checkout of the workspace`
- Outside a Cargo workspace:
  ``error: no Cargo workspace found: could not find `Cargo.toml` in `<dir>` or any parent directory``

Other git failures (`safe.directory`, corrupt objects) are reported in git's own words.

## CI: GitHub Actions

The recipe below lives at [`docs/ci/github-actions.yml`](docs/ci/github-actions.yml); copy
it into `.github/workflows/`. On every pull request a `plan` job computes the Affected
Crates between the fork point of the PR's base branch and the PR head, turns their names
into a job matrix with `jq`, and a `test` job runs once per crate. When nothing is affected
the `test` job is skipped rather than failed, because a matrix must not be empty.

Three details matter:

- `fetch-depth: 0` (or any fetch that brings in the base branch's history) so that the
  merge-base exists. A shallow clone fails with the "no merge-base" error above.
- `--base origin/<base branch>` rather than `--from`: the Base is the fork point, so a PR is
  not blamed for commits that landed on the base branch after it forked. With
  `actions/checkout` on `pull_request`, `HEAD` is the merge of the PR into its base, so
  `--to HEAD` yields exactly the PR's changes.
- `--format json` plus `jq '{crate: [.affected[].name]}'` is the matrix, and
  `jq '.affected | length'` is the count the `if:` tests; an empty Change Set gives
  `{"crate":[]}` and `0`.

```yaml
# Runs the test suite only for the crates a pull request affects.
# Copy this file into `.github/workflows/`, then adjust the install step and the
# test command to your repository.
name: Affected crates

on:
  pull_request:

permissions:
  contents: read

jobs:
  plan:
    name: Plan
    runs-on: ubuntu-latest
    outputs:
      matrix: ${{ steps.affected.outputs.matrix }}
      count: ${{ steps.affected.outputs.count }}
    steps:
      - uses: actions/checkout@v4
        with:
          # The Base is the merge-base of the base branch and HEAD, so the clone
          # must hold enough history to contain it. Full history is the simplest
          # way; a shallow clone fails with "no merge-base" instead of guessing.
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo affected
        # Once published: cargo install --locked haf-cargo-affected
        run: cargo install --locked --git <url of this repository>
      - name: Compute the Affected Crates
        id: affected
        env:
          BASE_REF: ${{ github.base_ref }}
        run: |
          report=$(cargo affected --base "origin/$BASE_REF" --to HEAD --format json)
          echo "$report"
          {
            echo "matrix=$(jq -c '{crate: [.affected[].name]}' <<<"$report")"
            echo "count=$(jq '.affected | length' <<<"$report")"
          } >> "$GITHUB_OUTPUT"

  test:
    name: Test ${{ matrix.crate }}
    needs: plan
    # Skip, rather than fail, when nothing is affected: a matrix must not be empty.
    # Equivalent: if: fromJSON(needs.plan.outputs.matrix).crate[0] != null
    if: needs.plan.outputs.count != '0'
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix: ${{ fromJSON(needs.plan.outputs.matrix) }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p ${{ matrix.crate }}
```

Outside GitHub the same shape works anywhere: `cargo affected --base origin/main --to HEAD`
prints the crates to test, and an empty output means there is nothing to do.

## Known behaviours

- `cargo metadata` rewrites a stale `Cargo.lock` on disk as a side effect. The Change Set
  is computed before the workspace is loaded, so the report is unaffected, but the working
  tree may differ after a run.
- When the workspace sits below the git root, files of the Change Set above the workspace
  root are neither mapped nor listed under `-v`; the mapping and the diagnostics cover the
  files under the workspace root.
- `via` may name a non-member path crate: `uses-foo` above reports `"via": ["foo"]` even
  though `foo` never appears in `affected`. The tree shows the same crate as a
  `(non-member)` root.
- In the tree, a member whose own files changed is a root even when its reason is
  `global` (a Global File changed in the same Change Set), so its Dependents are still
  explained; in the example above `core` is a root beside `Cargo.lock`.
- Roots of all kinds sort together by name in byte order, so `Cargo.lock` precedes `core`.
- `--charset` is accepted and ignored with `--format list|json`.
- Crates present in the Change Set but unknown to `cargo metadata` (deleted crates,
  `workspace.exclude`d crates that nothing depends on) are not reported; their files fall
  through to the crate-directory and unmapped rules.
- Precise `Cargo.lock` diffing (affecting only the crates that depend on a bumped external
  dependency), disabling the built-in Global Files and coloured output are out of scope for
  now.

## Name

`cargo-affected` on crates.io is an unrelated, coverage-based test selector. crates.io has
a flat namespace, so this package is `haf-cargo-affected`, prefixed by its author, while
the binary keeps the name `cargo-affected` and the subcommand stays `cargo affected`. The
two binaries clash on disk, so `cargo install` refuses to install both; nobody needs both
at once. The decision is recorded in
[ADR 0001](docs/adr/0001-crate-name-haf-cargo-affected.md).

## Development

`devenv shell` (or `direnv allow`) provides the toolchain and installs the git hooks. Then
`cargo build`, `cargo test`, `cargo clippy`; `nix build` builds the package and runs the
test suite inside the Nix sandbox. Commits follow Conventional Commits. The design notes
are in [`CONTEXT.md`](CONTEXT.md) and [`docs/adr/`](docs/adr/), and the feature tickets
that built the tool in [`.scratch/affected-crates/`](.scratch/affected-crates/).
