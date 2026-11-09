# 08: README and CI recipe

**What to build:** A README that lets someone adopt the tool from a cold start: install, the mental model (Change Set between a from and a to end, Changed versus Affected Crates), the flags, the three output formats with short examples, the configuration tables for Global Files, Extra Paths and ignore, and the relationship to the unrelated `cargo-affected` crate on crates.io (hence the package name). Includes a worked GitHub Actions example that runs the tool on a pull request, turns the JSON output into a job matrix with `jq`, and skips the test job when the matrix is empty. The Nix package builds and runs the binary with `--help`.

**Blocked by:** 05 (JSON output), 06 (Tree output).

**Status:** ready-for-agent

- [ ] Every flag accepted by the binary is documented and every documented flag is accepted
- [ ] Each config key in the README has a matching integration test fixture in the repo
- [ ] The GitHub Actions example runs the tool against the fork point of the PR base and produces an empty matrix without failing when nothing is affected
- [ ] `nix build` succeeds and the resulting binary answers `--help`
- [ ] The README uses the glossary vocabulary from `CONTEXT.md` and links to it
