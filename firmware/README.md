# hw-gauge-firmware

This firmware is for a Pico (RP2040) board with a ST7789V SPI display connected
to GP2 (SCLK) and GP3 (MOSI).

The [LilyGO T-Display RP2040] is a ready made board that is compatible with this
firmware.

## Building and flashing

Install build environment:

```sh
rustup target add thumbv6m-none-eabi
cargo install probe-run
```

Build & flash debug firmware:

```sh
env DEFMT_LOG=debug cargo r
```

Build & flash release firmware:

```sh
cargo rr
```

[LilyGO T-Display RP2040]: https://github.com/Xinyuan-LilyGO/LILYGO-T-display-RP2040
