flake: { config, lib, pkgs, self, ... }:
with lib; let
  cfg = config.services.hw-gauge-daemon;
in
{
  options.services.hw-gauge-daemon = {
    enable = mkEnableOption "Enable the hw-gauge daemon to display HW info";

    package = mkOption {
      type = types.package;
      default = flake.packages.${pkgs.system}.daemon;
    };
  };

  config = mkIf cfg.enable {
    systemd.services.hw-gauge-daemon = {
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/hw-gauge-daemon";
        DynamicUser = "yes";
        Group = "dialout";
        Restart = "on-failure";
      };
    };
  };
}

