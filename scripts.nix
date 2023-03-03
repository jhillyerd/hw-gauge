{ pkgs }: with pkgs; {
  firmware = {
    toolchain = writeScriptBin "firmware-toolchain" ''
      set -e
    '';

    ci = writeScriptBin "firmware-ci" ''
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

  daemon.linux.ci = writeScriptBin "linux-daemon-ci" ''
    set -e
    cd daemon/linux

    echo "::group::Checking Rust formatting"
    cargo fmt --check
    echo "::endgroup::"

    echo "::group::Build and lint"
    cargo clippy -- -D warnings
    echo "::endgroup::"
  '';
}
