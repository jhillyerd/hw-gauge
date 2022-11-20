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

After building the executable with cargo, create a `hw-gauge` folder in
`Program Files`, and copy `hw-gauge-winsvc.exe` into it.

Then run the following command from an Administrator PowerShell prompt:

```powershell
new-service -name "hw-gauge-winsvc" -binarypathname "C:\Program Files\hw-gauge\hw-gauge-winsvc.exe"
```

## firmware

Firmware for [LilyGO T-Display RP2040] boards.  It should be relatively easy to
modify for a regular Pi Pico with a ST7789 SPI display.

## Notes

This project does not use a cargo workspace as building for different targets
does not work well within them.  This may change after
[#9030](https://github.com/rust-lang/cargo/pull/9030) is merged.

[LilyGO T-Display RP2040]: https://github.com/Xinyuan-LilyGO/LILYGO-T-display-RP2040
