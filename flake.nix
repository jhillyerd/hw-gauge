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

          scripts = {
            firmware = {
              toolchain = pkgs.writeScriptBin "firmware-toolchain" ''
                set -e
              '';

              ci = pkgs.writeScriptBin "firmware-ci" ''
                set -e
                cd firmware

                echo "::group::Checking Rust formatting"
                cargo fmt --check
                echo "::endgroup::"

                echo "::group::Build and lint"
                cargo clippy -- -D warnings
                echo "::endgroup::"
              '';
            };

            daemon.linux.ci = pkgs.writeScriptBin "linux-daemon-ci" ''
              set -e
              cd daemon/linux

              echo "::group::Checking Rust formatting"
              cargo fmt --check
              echo "::endgroup::"

              echo "::group::Build and lint"
              cargo clippy -- -D warnings
              echo "::endgroup::"
            '';
          };
        in
        {
          devShells.default = pkgs.mkShell {
            buildInputs = with pkgs; [
              flip-link
              gdb
              glibc
              libusb
              openssl
              pkg-config
              rustup

              scripts.firmware.toolchain
              scripts.firmware.ci
              scripts.daemon.linux.ci
            ];
          };
        });
}
