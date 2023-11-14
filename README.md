# hw-gauge

A rust powered CPU and memory monitor for Linux and Windows systems.

The device displays a bar graph indicating both the all-cores average (on the
left) and the peak core load, as well as the 15 second average in numeric form.

The graphs are updated once per second, but the CPU bars have animated falloff
to make it more visually appealing.

![hw-gauge on the Lilygo](https://github.com/jhillyerd/hw-gauge/blob/main/images/lilygo.jpg?raw=true)

## daemon/linux

A simple Linux daemon to send CPU info to the device.

## daemon/windows

Windows service to send CPU info to the device.

### Building

If you don't already have a Rust MSVC toolchain installed, install the
VisualStudio Build Tools with the `Desktop development with C++` option
enabled.  Then install `rustup`, and finally close+reopen PowerShell to load
the updated PATH:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
winget install Rustlang.Rustup
exit
```

Build the service executable:

```powershell
cd daemon\windows
cargo build -r
```

### Installation

After building the executable, create a `hw-gauge` folder in
`C:\Program Files`, and copy `target\release\hw-gauge-winsvc.exe` into it;
without creating the target and release directories.

Run the following command from an Administrator PowerShell prompt to register
the service:

```powershell
new-service -name "hw-gauge-winsvc" -binarypathname "C:\Program Files\hw-gauge\hw-gauge-winsvc.exe"
```

You may then use `services.msc` to start the newly added service.

## firmware

Firmware for [LilyGO T-Display RP2040] boards.  It should be relatively easy to
modify for a regular Pi Pico with a ST7789 SPI display.


[LilyGO T-Display RP2040]: https://github.com/Xinyuan-LilyGO/LILYGO-T-display-RP2040
