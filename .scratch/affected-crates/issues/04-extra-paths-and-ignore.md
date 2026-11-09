# 04: Per-crate Extra Paths and ignore globs

**What to build:** A crate can declare, in `[package.metadata.affected]`, `extra-paths`: gitignore-style globs relative to its own directory that make it a Changed Crate when matched, even outside its directory (for example a build script reading a sibling `proto` directory). It can also declare `ignore`: globs for files inside its directory that must not count as changes (documentation, readme). Mapping order per file is: Global File, then Extra Path of any crate, then deepest containing crate minus its ignore globs, then unmapped. Ignore defaults to nothing, so no false negatives without explicit configuration.

**Blocked by:** 01 (Walking skeleton).

**Status:** ready-for-agent

- [ ] A fixture where crate `codegen` declares `extra-paths = ["../proto/**"]`: editing a file under `proto/` prints `codegen`
- [ ] A fixture where crate `lib` declares `ignore = ["README.md", "docs/**"]`: editing its readme prints nothing, editing its source prints `lib`
- [ ] An Extra Path pointing inside another crate's directory makes both crates Changed
- [ ] A crate with no metadata table behaves exactly as before
- [ ] Malformed metadata (wrong type for `extra-paths`) is a clear error naming the crate
