{
  description = "hw-gauge hardware monitor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    let
      overlays = [
        (import rust-overlay)
        # Build Rust toolchain with helpers from rust-overlay
        (self: super: {
          rustToolchain = super.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        })
      ];
    in
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = import nixpkgs { inherit overlays system; };

          scripts.firmware = {
            toolchain = pkgs.writeScriptBin "firmware-toolchain" ''
              # TODO remove, rust-overlay does the work now
              true
            '';

            ci = pkgs.writeScriptBin "firmware-ci" ''
              set -e
              unset LD_LIBRARY_PATH
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
              glibc
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
