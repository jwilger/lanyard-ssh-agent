{
  description = "Lanyard SSH agent switching proxy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
    }:
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ]
      (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          toolchain = fenix.packages.${system}.stable.withComponents [
            "cargo"
            "clippy"
            "rustc"
            "rustfmt"
          ];
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          package = rustPlatform.buildRustPackage {
            pname = "lanyard-ssh-agent";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
          };
        in
        {
          packages = {
            default = package;
            lanyard-ssh-agent = package;
          };
          apps.default = flake-utils.lib.mkApp { drv = package; };
          formatter = pkgs.nixfmt-tree;
          checks.package = package;
          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              actionlint
              cargo-deny
              cargo-mutants
              git
              just
              nixfmt-tree
              openssh
              toolchain
            ];
            shellHook = ''
              export CARGO_HOME="$PWD/.dependencies/cargo"
              export CARGO_TARGET_DIR="$PWD/.dependencies/target"
            '';
          };
        }
      );
}
