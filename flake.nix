{
  description = "hw-gauge hardware monitor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    let
      inherit (nixpkgs) lib;
      inherit (flake-utils.lib) eachSystem system;

      overlays = [
        (import rust-overlay)
        # Build Rust toolchain with helpers from rust-overlay
        (self: super: {
          rustToolchain = super.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        })
      ];
    in
    {
      nixosModules.hw-gauge-daemon = import ./module.nix self;
      nixosModules.default = self.nixosModules.hw-gauge-daemon;
    } //
    eachSystem [ system.x86_64-linux ]
      (system:
        let
          pkgs = import nixpkgs { inherit overlays system; };

          code = import ./. { inherit system pkgs crane; };

          scripts = import ./scripts.nix { inherit pkgs; };
        in
        {
          packages = {
            daemon = code.daemon;
          };

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
