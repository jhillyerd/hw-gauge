{ pkgs ? import <nixpkgs> { } }:
with pkgs;
mkShell {
  buildInputs = [
    gdb
    libusb
    openssl
    pkg-config
    rustup
  ];
}
