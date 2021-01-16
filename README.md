# hw-cpu
Rust powered CPU monitor... maybe?

## daemon

Windows daemon to send CPU info to device.

## firmware

Firmware for STMF103 bluepill.

## Notes

This project does not use a cargo workspace as building for different targets
does not work well within them.  This may change after
[#9030](https://github.com/rust-lang/cargo/pull/9030) is merged.
