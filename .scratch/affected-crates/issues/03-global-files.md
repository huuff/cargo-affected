# 03: Global Files

**What to build:** A change to a Global File makes every workspace member an Affected Crate. Built-in Global Files are the workspace manifest, the lockfile, `rust-toolchain` and `rust-toolchain.toml`, and anything under the `.cargo` directory at the workspace root. Users extend the set with gitignore-style globs in `[workspace.metadata.affected].global`, matched relative to the workspace root. Global File matching runs before crate mapping, so a root package does not claim the workspace manifest as its own file. Each affected crate carries one reason with precedence `global` > `changed` > `dependent`; this ticket introduces the reason so later output formats can show it.

**Blocked by:** 01 (Walking skeleton).

**Status:** ready-for-agent

- [ ] Editing `Cargo.lock` in a fixture prints every member
- [ ] Editing a file under `.cargo/` prints every member
- [ ] A fixture declaring `global = ["ci/**"]` prints every member when a file under `ci/` changes, and the built-in globals still apply
- [ ] A fixture with a root package: editing only the workspace manifest yields reason `global` for every member, not `changed` for the root package
- [ ] `--changed-only` still prints every member when a Global File changed
- [ ] Unit tests cover the precedence rule when a crate is simultaneously global, changed and dependent
