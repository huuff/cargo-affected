{
  description = "Cargo subcommand that lists workspace crates affected by a change";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/package.nix { };
      });

      overlays.default = final: _prev: {
        cargo-affected = final.callPackage ./nix/package.nix { };
      };

      # Uncomment if the project ships NixOS / Home Manager modules:
      # nixosModules.default = ./nix/module.nix;
      # homeManagerModules.default = ./nix/hm-module.nix;

      checks = forAllSystems (pkgs: {
        package = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });
    };
}
