# hw-gauge

Rust powered CPU and memory monitor.

![hw-gauge assembled photo](https://github.com/jhillyerd/hw-gauge/blob/main/images/assembled.jpg?raw=true)

## daemon/linux

A simple Linux daemon to send CPU info to the device.

## daemon/windows

Windows service to send CPU info to the device.

After building the executable with cargo, create a `hw-gauge` folder in `Program Files`, and copy
`hw-gauge-winsvc.exe` into it.

Then run the following command from an Administrator PowerShell prompt:

```powershell
new-service -name "hw-gauge-winsvc" -binarypathname "C:\Program Files\hw-gauge\hw-gauge-winsvc.exe"
```

## firmware

Firmware for STMF103 bluepill boards.

## Notes

This project does not use a cargo workspace as building for different targets
does not work well within them.  This may change after
[#9030](https://github.com/rust-lang/cargo/pull/9030) is merged.

## Additional images

![hw-gauge case design](https://github.com/jhillyerd/hw-gauge/blob/main/images/case-design.png?raw=true)

![hw-gauge internals](https://github.com/jhillyerd/hw-gauge/blob/main/images/case-internals.jpg?raw=true)
