# cargo-affected

Cargo subcommand that lists workspace crates affected by a change. Rust binary; `src/main.rs` is the entry point.

## Working in the repo

- Enter the dev shell with `devenv shell` (or `direnv allow`). Git hooks install on shell entry; never bypass them with `--no-verify`.
- Build/test: `cargo build`, `cargo test`, `cargo clippy`. `devenv test` runs the scaffold check (`nix flake check --no-build`).
- Nix package: `nix build` (uses `nix/package.nix`; keep `Cargo.lock` committed, it is the lockfile for `buildRustPackage`).
- Commits follow Conventional Commits (`feat:`, `fix:`, `chore:`, ...); the commitizen hook enforces it.
- devenv `scripts.*` with real logic: use nushell (`package = pkgs.nushell`); CLI args need `def --wrapped main [...args]`, interpolation is `$"(...)"`.
