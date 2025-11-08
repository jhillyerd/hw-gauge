{
  pkgs,
  rustPlatform,
  ...
}:
{
  daemon = rustPlatform.buildRustPackage {
    pname = "hw-gauge-daemon";
    version = "0.1.0";

    src = ./.;
    cargoLock = {
      lockFile = ./Cargo.lock;
    };
    buildAndTestSubdir = "daemon/linux";

    buildInputs = with pkgs; [ systemd ];
    nativeBuildInputs = with pkgs; [ pkg-config ];
  };
}
