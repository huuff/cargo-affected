{
  lib,
  rustPlatform,
  git,
}:

rustPlatform.buildRustPackage {
  pname = "haf-cargo-affected";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  cargoLock.lockFile = ../Cargo.lock;

  # The integration tests `git init` fixture workspaces and run the binary, which
  # shells out to git.
  nativeCheckInputs = [ git ];

  meta = {
    description = "Cargo subcommand that lists workspace crates affected by a change";
    license = lib.licenses.wtfpl;
    mainProgram = "cargo-affected";
  };
}
