{ system, pkgs, crane, ... }:
let
  craneLib = crane.lib.${system};
  version = "0.1.0";
in
rec
{
  daemon = craneLib.buildPackage {
    inherit version;

    pname = "hw-gauge-daemon";

    src = craneLib.cleanCargoSource ./.;
    cargoExtraArgs = "-p linux";

    buildInputs = with pkgs; [ systemd ];

    nativeBuildInputs = with pkgs; [ pkg-config ];
  };
}
