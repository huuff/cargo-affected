# 1. Publish as `haf-cargo-affected`, install as `cargo affected`

Date: 2026-11-09

## Status

Accepted

## Context

The natural crate name `cargo-affected` is taken on crates.io by an unrelated
tool (max-sixty/cargo-affected, a coverage-based test selector). crates.io has a
flat, first-come namespace with no owner scopes, so the name cannot be shared.
We intend to publish.

## Decision

Package name `haf-cargo-affected`, namespaced by author prefix. The binary is
still named `cargo-affected` (`[[bin]] name`), so Cargo discovers it as
`cargo affected`.

## Consequences

- Publishable without renaming the tool or the subcommand users type.
- The binary clashes on disk with the other crate's binary; `cargo install`
  refuses to install both. We accept this: nobody needs both tools at once.
- Renaming after the first publish means abandoning the crates.io name, so this
  decision is effectively permanent.
