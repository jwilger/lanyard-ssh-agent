{
  description = "Lanyard SSH agent switching proxy";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
      home-manager,
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
          testPackage = pkgs.writeShellScriptBin "lanyard-ssh-agent" ''
            exit 0
          '';
          homeConfiguration = home-manager.lib.homeManagerConfiguration {
            inherit pkgs;
            modules = [
              self.homeManagerModules.default
              {
                home = {
                  username = "lanyard-test";
                  homeDirectory = "/home/lanyard-test";
                  stateVersion = "26.05";
                };
                programs.lanyard-ssh-agent = {
                  enable = true;
                  package = testPackage;
                };
              }
            ];
          };
          defaultHomeConfiguration = home-manager.lib.homeManagerConfiguration {
            inherit pkgs;
            modules = [
              self.homeManagerModules.default
              {
                home = {
                  username = "lanyard-test";
                  homeDirectory = "/home/lanyard-test";
                  stateVersion = "26.05";
                };
              }
            ];
          };
        in
        {
          packages = {
            default = package;
            lanyard-ssh-agent = package;
          };
          apps.default = flake-utils.lib.mkApp { drv = package; };
          formatter = pkgs.nixfmt-tree;
          checks = {
            package = package;
            home-manager-module =
              assert defaultHomeConfiguration.config.programs.lanyard-ssh-agent.package == package;
              pkgs.runCommand "lanyard-home-manager-module" { } ''
                test -x ${homeConfiguration.config.home.path}/bin/lanyard-ssh-agent
                touch "$out"
              '';
          };
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
      )
    // {
      homeManagerModules.default = import ./nix/home-manager-module.nix self;
    };
}
