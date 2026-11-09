# 2. User globs are anchored at their root

Date: 2026-11-09

## Status

Accepted

## Context

Three settings take gitignore-style globs: `[workspace.metadata.affected].global`
(Global Files), and `extra-paths` and `ignore` in a crate's
`[package.metadata.affected]`. gitignore has two matching modes: a pattern with
no `/` (other than a trailing one) matches at any depth, while a pattern with a
`/` is anchored at the directory holding the `.gitignore`. `globset`, which we
build on, has no notion of anchoring: `*.toml` matches only at the root and a
leading `/` is a literal that matches nothing. Ticket 03 shipped `global` with
plain `globset` semantics and left the decision for ticket 04, which reuses the
glob machinery.

## Decision

Every glob is anchored at its root, whatever its shape. The root is the
workspace root for `global` and the crate's own directory for `extra-paths` and
`ignore`. `*` and `?` stay within one path component, `**` spans directories,
and a leading `/` or `./` is accepted and means the same as no prefix.
`extra-paths` may start with `../`; they are rebased onto the workspace root
lexically, and a glob that climbs above it is an error.

So `*.toml` matches `deny.toml` but not `ci/deny.toml`; write `**/*.toml` to
match at any depth, and `README.md` in a crate's `ignore` leaves
`src/README.md` alone.

## Consequences

- One rule for all three settings, explained in one sentence in the README.
- Predictable: a pattern never matches more than it literally spells out, so
  a short `ignore` glob cannot silently hide changes deeper in the crate.
- Users porting a `.gitignore` line like `*.md` must add `**/` themselves.
- Only files under the workspace root are ever mapped, so an Extra Path that
  escapes it could never match; rejecting it surfaces the mistake instead of
  silently never firing.
