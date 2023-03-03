{ system, pkgs, crane, ... }:
let
  craneLib = crane.lib.${system};
  version = "0.1.0";
in rec
{
  shared = craneLib.buildPackage {
    inherit version;

    pname = "hw-gauge-shared";

    src = craneLib.cleanCargoSource ./shared;
  };

  daemon = craneLib.buildPackage {
    inherit version;

    pname = "hw-gauge-daemon";

    src = craneLib.cleanCargoSource ./daemon;
    cargoExtraArgs = "-p linux";

    buildInputs = [ shared ];
  };
}
