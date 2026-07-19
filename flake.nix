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
            printf '%s|%s\n' "$SSH_AUTH_SOCK" "$*" >> "$LANYARD_TEST_LOG"
            if [ -n "''${LANYARD_TEST_FAIL-}" ]; then
              exit 1
            fi
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
                  upstream = ''/home/lanyard-test/Agent 100% "socket"'';
                };
                programs.bash.enable = true;
                programs.zsh.enable = true;
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
            home-manager-systemd-service =
              let
                service = "${homeConfiguration.config.home-files}/.config/systemd/user/lanyard-ssh-agent.service";
              in
              pkgs.runCommand "lanyard-home-manager-systemd-service" { } ''
                grep -F -- '"serve" "--upstream" "/home/lanyard-test/Agent 100%% \"socket\""' ${service}
                grep -F -- "Restart=on-failure" ${service}
                grep -F -- "WantedBy=default.target" ${service}
                touch "$out"
              '';
            home-manager-shell-integration =
              let
                bashProfile = "${homeConfiguration.config.home-files}/.profile";
                zshEnvironment = "${homeConfiguration.config.home-files}/.zshenv";
              in
              pkgs.runCommand "lanyard-home-manager-shell-integration" { } ''
                export HOME="$TMPDIR/home"
                export LANYARD_TEST_LOG="$TMPDIR/lanyard.log"
                export SSH_AUTH_SOCK="$TMPDIR/forwarded-agent.sock"
                export SSH_CONNECTION="client.example 22 server.example 2222"
                export XDG_RUNTIME_DIR="$TMPDIR/runtime"

                source ${bashProfile}

                test "$(cat "$LANYARD_TEST_LOG")" = "$TMPDIR/forwarded-agent.sock|register $TMPDIR/forwarded-agent.sock"
                test "$SSH_AUTH_SOCK" = "$TMPDIR/runtime/lanyard-ssh-agent/agent.sock"

                export LANYARD_TEST_FAIL=1
                export SSH_AUTH_SOCK="$TMPDIR/second-forwarded-agent.sock"
                source ${bashProfile}
                test "$SSH_AUTH_SOCK" = "$TMPDIR/runtime/lanyard-ssh-agent/agent.sock"

                unset LANYARD_TEST_FAIL
                export SSH_AUTH_SOCK="$TMPDIR/zsh-forwarded-agent.sock"
                ${pkgs.zsh}/bin/zsh -c '
                  source ${zshEnvironment}
                  test "$SSH_AUTH_SOCK" = "$XDG_RUNTIME_DIR/lanyard-ssh-agent/agent.sock"
                '
                tail -n 1 "$LANYARD_TEST_LOG" | grep -F -- "$TMPDIR/zsh-forwarded-agent.sock|register $TMPDIR/zsh-forwarded-agent.sock"
                touch "$out"
              '';
            home-manager-ssh-integration =
              let
                sshConfig = "${homeConfiguration.config.home-files}/.ssh/config";
              in
              pkgs.runCommand "lanyard-home-manager-ssh-integration" { } ''
                grep -F -- "Host *" ${sshConfig}
                grep -F -- "IdentityAgent SSH_AUTH_SOCK" ${sshConfig}
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
