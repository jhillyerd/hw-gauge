# hw-cpu

Rust powered CPU and memory monitor.

![hw-cpu assembled photo](https://github.com/jhillyerd/hw-cpu/blob/photos/images/assembled.jpg?raw=true)

## daemon

Windows daemon to send CPU info to device.

## firmware

Firmware for STMF103 bluepill.

## Notes

This project does not use a cargo workspace as building for different targets
does not work well within them.  This may change after
[#9030](https://github.com/rust-lang/cargo/pull/9030) is merged.

## Additional images

![hw-cpu case design](https://github.com/jhillyerd/hw-cpu/blob/photos/images/case-design.png?raw=true)

![hw-cpu internals](https://github.com/jhillyerd/hw-cpu/blob/photos/images/case-internals.jpg?raw=true)