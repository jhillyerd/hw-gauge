# hw-cpu-firmware

This firmware is for a bluepill (STM32F103C8) board with an I2C display
connected to B10 (SCL) and B11 (SDA).

Install build environment:

```sh
rustup target add thumbv7m-none-eabi
cargo install probe-run
```

Build & flash debug firmware:

```sh
env DEFMT_LOG=debug cargo run
```

Build & flash release firmware:

```sh
cargo rr
```
