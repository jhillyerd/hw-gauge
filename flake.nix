{
  description = "hw-gauge hardware monitor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = import nixpkgs { inherit system; };

          scripts.firmware = {
            toolchain = pkgs.writeScriptBin "firmware-toolchain" ''
              set -e
              cd firmware

              rustup target add thumbv6m-none-eabi
            '';

            ci = pkgs.writeScriptBin "firmware-ci" ''
              set -e
              cd firmware

              echo "Checking Rust formatting..."
              cargo fmt --check

              echo "Building firmware..."
              cargo build --release
            '';
          };
        in
        {
          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              gdb
              libusb
              openssl
              pkg-config
              rustup

              scripts.firmware.toolchain
              scripts.firmware.ci
            ];
          };
        });
}
