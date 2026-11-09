{
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  pname = "haf-cargo-affected";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  cargoLock.lockFile = ../Cargo.lock;

  meta = {
    description = "Cargo subcommand that lists workspace crates affected by a change";
    license = lib.licenses.mit;
    mainProgram = "cargo-affected";
  };
}
