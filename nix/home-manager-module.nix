self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.lanyard-ssh-agent;
  escapeSystemdExecArg =
    argument: lib.replaceStrings [ "%" "$" ] [ "%%" "$$" ] (builtins.toJSON (toString argument));
  escapeSystemdExecArgs = lib.concatMapStringsSep " " escapeSystemdExecArg;
in
{
  options.programs.lanyard-ssh-agent = {
    enable = lib.mkEnableOption "Lanyard SSH agent switching proxy";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "inputs.lanyard.packages.${pkgs.system}.default";
      description = "The Lanyard package to install.";
    };

    upstream = lib.mkOption {
      type = lib.types.str;
      default = "${config.home.homeDirectory}/.1password/agent.sock";
      defaultText = lib.literalExpression ''"\${config.home.homeDirectory}/.1password/agent.sock"'';
      example = "/run/user/1000/onepassword/agent.sock";
      description = "The durable local SSH agent socket used as Lanyard's final fallback.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    systemd.user.services.lanyard-ssh-agent = lib.mkIf pkgs.stdenv.isLinux {
      Install.WantedBy = [ "default.target" ];
      Unit.Description = "Lanyard SSH agent switching proxy";
      Service = {
        ExecStart = escapeSystemdExecArgs [
          "${cfg.package}/bin/lanyard-ssh-agent"
          "serve"
          "--upstream"
          cfg.upstream
        ];
        Restart = "on-failure";
      };
    };
  };
}
