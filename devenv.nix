# Add `pkgs`, `lib`, etc. to the lambda args when first needed (deadnix rejects
# unused args, statix rejects empty `{ ... }` patterns — use `_:` if no
# args remain).
_:

{
  # Extra tools in the shell, on top of what `languages.*` bring in.
  packages = [
    # Uncomment if the app needs runtime secrets (see "Runtime secrets" in SKILL.md):
    # pkgs.sops
    # pkgs.age
  ];

  languages.rust.enable = true;
  languages.nix.enable = true;

  git-hooks.hooks = {
    # --- secrets: never commit credentials ---
    ripsecrets.enable = true; # scans staged changes for API keys/tokens
    detect-private-keys.enable = true;

    # --- hygiene ---
    check-added-large-files.enable = true;
    check-merge-conflicts.enable = true;
    end-of-file-fixer.enable = true;
    trim-trailing-whitespace.enable = true;

    # --- static analysis: nix ---
    nixfmt.enable = true; # RFC 166 style; nixfmt >= 1.0 (nixfmt-rfc-style is the deprecated alias)
    statix.enable = true;
    deadnix.enable = true;

    # --- static analysis: shell ---
    shellcheck.enable = true;
    shfmt.enable = true;

    # --- static analysis: rust ---
    clippy.enable = true;
    rustfmt.enable = true;

    # --- commit messages: Conventional Commits (feat:, fix:, chore:, ...) ---
    commitizen.enable = true;
  };

  # Uncomment if the app needs runtime secrets: decrypts secrets/dev.yaml with
  # sops and execs the given command with the secrets as environment variables.
  # scripts.with-secrets = {
  #   description = "Run a command with sops-decrypted secrets in the env";
  #   exec = ''
  #     exec sops exec-env secrets/dev.yaml "$*"
  #   '';
  # };

  enterTest = ''
    nix flake check --no-build
  '';
}
