self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.lanyard-ssh-agent;
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
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
  };
}
