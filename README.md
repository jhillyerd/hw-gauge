# hw-cpu

Rust powered CPU and memory monitor.

![hw-cpu assembled photo](https://github.com/jhillyerd/hw-cpu/blob/main/images/assembled.jpg?raw=true)

## daemon/linux

A simple Linux daemon to send CPU info to the device.

## daemon/windows

Windows service to send CPU info to the device.

After building the executable with cargo, create a `hw-cpu` folder in `Program Files`, and copy
`hw-cpu-winsvc.exe` into it.

Then run the following command from an Administrator PowerShell prompt:

```powershell
new-service -name "hw-cpu-winsvc" -binarypathname "C:\Program Files\hw-cpu\hw-cpu-winsvc.exe"
```

## firmware

Firmware for STMF103 bluepill.

## Notes

This project does not use a cargo workspace as building for different targets
does not work well within them.  This may change after
[#9030](https://github.com/rust-lang/cargo/pull/9030) is merged.

## Additional images

![hw-cpu case design](https://github.com/jhillyerd/hw-cpu/blob/main/images/case-design.png?raw=true)

![hw-cpu internals](https://github.com/jhillyerd/hw-cpu/blob/main/images/case-internals.jpg?raw=true)