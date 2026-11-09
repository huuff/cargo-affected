# 07: Diagnostics and errors

**What to build:** `-v` prints to stderr, for every file in the Change Set, which crate it mapped to and why (crate directory, Extra Path, Global File, ignored, unmapped), so a user can see why CI did or did not run a crate. Without `-v`, stderr carries only errors. Failure modes are explicit: a `--base` or `--from` ref that does not exist, or a base branch that shares no history with HEAD (shallow clone), exits non-zero with a message that names the ref and suggests fetching more history. The tool never silently falls back to reporting every crate. Running outside a git repository or outside a Cargo workspace is also a clear error.

**Blocked by:** 03 (Global Files), 04 (Extra Paths and ignore).

**Status:** ready-for-agent

- [ ] Without `-v`, a successful run writes nothing to stderr
- [ ] With `-v`, each changed file appears on stderr with its mapping outcome, including an unmapped repository-root file and an ignored file
- [ ] `--base does-not-exist` exits non-zero and the message names the ref
- [ ] In a shallow clone with no merge-base, the error suggests fetching more history and no crates are printed
- [ ] Neither `origin/main` nor `main` existing, with no `--base`, is an error that says how to pass a base
- [ ] Running in a directory that is not a Cargo workspace is a clear error
