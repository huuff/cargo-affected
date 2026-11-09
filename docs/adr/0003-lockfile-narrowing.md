# 3. A lockfile change is narrowed to Locked Dependencies

Date: 2026-11-09

## Status

Accepted

## Context

`Cargo.lock` was a built-in Global File: any change to it made every workspace
member an Affected Crate. On a 68-crate workspace that bumps a crate version
after every fix, four of five real commits touched the lockfile and reported all
68 crates, so the tool narrowed nothing. The lockfile changes for two unrelated
reasons: a third-party package moved, or a path package's own entry was
rewritten (its version, its dependency list).

## Decision

The lockfile is no longer a built-in Global File. When it is in the Change Set,
the lockfile at the Base is compared with the inspected one, and a path package
becomes a Changed Crate, with `Cargo.lock` as its changed file, when:

- a third-party package it reaches through third-party packages only is new,
  has another checksum, or depends on a different set of packages; or
- its own set of direct third-party dependencies differs.

Entries of path packages are otherwise not compared. A version bump of a member
is already a change to its `Cargo.toml`, and its Dependents follow through the
dependency graph, so the lockfile adds nothing there.

The outcome is reported with the existing reason `changed` rather than a new
one: the JSON document keeps its shape, and the tree needs no new kind of root.
`-v` names the Locked Dependencies.

The lockfile falls back to a Global File when the change cannot be narrowed: it
is absent at either end, does not parse, a dependency reference is ambiguous, or
it differs outside `[[package]]` (format `version`, `[metadata]`, unused
patches). A user glob `global = ["Cargo.lock"]` restores the old behaviour.

## Consequences

- A dependency update affects only the crates that can use the package; a
  member version bump affects only that member and its Dependents.
- The lockfile is feature- and target-agnostic, so a crate is claimed for a
  package behind a feature it does not enable. This errs towards running more.
- A lockfile change that touches no Locked Dependency (a comment) affects
  nobody, where it used to affect everybody.
- A new dependency, `toml`, to parse the lockfile.
- The report cannot say which package changed except under `-v`; a `locked`
  field per crate would be the place if JSON consumers need it.
