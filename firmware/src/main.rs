#![no_main]
#![no_std]

use defmt::error;
use defmt_rtt as _;
use panic_probe as _;

mod gfx;
mod io;
mod perf;

#[rtic::app(
    device = rp_pico::pac,
    peripherals = true,
    dispatchers = [ PIO0_IRQ_0, PIO0_IRQ_1, PIO1_IRQ_0, PIO1_IRQ_1 ],
)]
mod app {
    use crate::{
        gfx, io,
        perf::{self, FramesDeque},
    };
    use cortex_m::asm;
    use defmt::{debug, error, info, warn};
    use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
    use embedded_hal::{digital::v2::OutputPin, spi::MODE_0};
    use fugit::{ExtU64, RateExtU32};
    use postcard;
    use rp_pico::hal::{self, clocks::Clock, usb, watchdog::Watchdog};
    use shared::{message, message::PerfData};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the board crystal.
    const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

    // Duration to illuninate status LED upon data RX.
    const STATUS_LED_MS: u64 = 50;

    // Delay from no data received to blanking the screen.
    const BLANK_SCREEN_SECS: u64 = 30;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type SysMono = rp2040_monotonic::Rp2040Monotonic;

    // LED blinks on USB activity.
    type ActivityLED = hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>;

    // ST7789V IPS screen, aka T-Display.
    type Display = mipidsi::Display<
        display_interface_spi::SPIInterface<
            hal::Spi<hal::spi::Enabled, hal::pac::SPI0, 8>,
            hal::gpio::Pin<hal::gpio::pin::bank0::Gpio1, hal::gpio::Output<hal::gpio::PushPull>>,
            hal::gpio::Pin<hal::gpio::pin::bank0::Gpio5, hal::gpio::Output<hal::gpio::PushPull>>,
        >,
        mipidsi::models::ST7789,
        hal::gpio::Pin<hal::gpio::pin::bank0::Gpio0, hal::gpio::Output<hal::gpio::PushPull>>,
    >;

    #[shared]
    struct Shared {
        // Queue of perf data frames to display.
        frames: FramesDeque,

        serial: io::Serial,
        display: Display,

        // Blinks ActivityLED briefly when set true.
        pulse_led: bool,

        // Previously received perf data message.
        prev_perf: Option<PerfData>,

        // Spawn handle for no data received timeouts.
        timeout_handle: Option<no_data_timeout::SpawnHandle>,
    }

    #[local]
    struct Local {
        led: crate::app::ActivityLED,
    }

    #[init(local = [usb_bus: Option<UsbBusAllocator<usb::UsbBus>> = None])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("RTIC init started");

        // Soft-reset does not release the hardware spinlocks.
        // Release them now to avoid a deadlock after debug or watchdog reset.
        unsafe {
            hal::sio::spinlock_reset();
        }

        // Setup clock & timer.
        let mut resets = ctx.device.RESETS;
        let mut watchdog = Watchdog::new(ctx.device.WATCHDOG);
        let clocks = hal::clocks::init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            ctx.device.XOSC,
            ctx.device.CLOCKS,
            ctx.device.PLL_SYS,
            ctx.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let mono = SysMono::new(ctx.device.TIMER);
        let mut delay =
            cortex_m::delay::Delay::new(ctx.core.SYST, clocks.system_clock.freq().to_Hz());

        // Setup status LED.
        let sio = hal::Sio::new(ctx.device.SIO);
        let pins = hal::gpio::Pins::new(
            ctx.device.IO_BANK0,
            ctx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );
        let mut led: ActivityLED = pins.gpio25.into_push_pull_output();
        led.set_low().unwrap();

        // Setup USB bus and serial port device.
        *ctx.local.usb_bus = Some(UsbBusAllocator::new(usb::UsbBus::new(
            ctx.device.USBCTRL_REGS,
            ctx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        )));
        let port = usbd_serial::SerialPort::new(ctx.local.usb_bus.as_ref().unwrap());
        let usb_dev = UsbDeviceBuilder::new(
            ctx.local.usb_bus.as_ref().unwrap(),
            UsbVidPid(USB_VENDOR_ID, USB_PRODUCT_ID),
        )
        .manufacturer("JHillyerd")
        .product("System monitor")
        .serial_number("TEST")
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

        // Setup SPI for onboard T-Display.
        // TODO confirm correct spi pins in use?
        let _ = pins.gpio2.into_mode::<hal::gpio::FunctionSpi>();
        let _ = pins.gpio3.into_mode::<hal::gpio::FunctionSpi>();
        let spi = hal::Spi::<_, _, 8>::new(ctx.device.SPI0).init(
            &mut resets,
            125.MHz(),
            16.MHz(),
            &MODE_0,
        );

        // Setup display.
        let cs_pin = pins.gpio5.into_push_pull_output();
        let dc_pin = pins.gpio1.into_push_pull_output();
        let rst_pin = pins.gpio0.into_push_pull_output();
        let display_if = display_interface_spi::SPIInterface::new(spi, dc_pin, cs_pin);
        let mut display = mipidsi::builder::Builder::st7789(display_if)
            .with_display_size(240, 135)
            .init(&mut delay, Some(rst_pin))
            .expect("display initializes");
        display.clear(Rgb565::BLACK).expect("display clears");

        // Start tasks.
        pulse_led::spawn().unwrap();
        show_perf::spawn().unwrap();

        info!("RTIC init completed");

        (
            Shared {
                frames: FramesDeque::new(),
                serial: io::Serial::new(usb_dev, port),
                display,
                pulse_led: false,
                prev_perf: None,
                timeout_handle: Some(no_data_timeout::spawn_after(10.secs(), false).unwrap()),
            },
            Local { led },
            init::Monotonics(mono),
        )
    }

    #[idle()]
    fn idle(_ctx: idle::Context) -> ! {
        loop {
            cortex_m::asm::nop();
        }
    }

    #[task(shared = [pulse_led], local = [led])]
    fn pulse_led(ctx: pulse_led::Context) {
        let mut pulse_led = ctx.shared.pulse_led;
        let led = ctx.local.led;

        pulse_led.lock(|pulse_led| {
            if *pulse_led {
                led.set_low().ok();
                *pulse_led = false;
            } else {
                led.set_high().ok();
            }
        });

        // Clear LED after a delay.
        pulse_led::spawn_after(STATUS_LED_MS.millis()).unwrap();
    }

    #[task(priority = 4, binds = USBCTRL_IRQ, shared = [serial, pulse_led])]
    fn usb_event(ctx: usb_event::Context) {
        let usb_event::SharedResources { serial, pulse_led } = ctx.shared;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task(priority = 3, shared = [timeout_handle])]
    fn handle_packet(mut ctx: handle_packet::Context, mut buf: [u8; io::BUF_BYTES]) {
        let msg: Result<message::FromHost, _> = postcard::from_bytes_cobs(&mut buf);
        match msg {
            Ok(msg) => {
                debug!("Rx message: {:?}", msg);
                match msg {
                    message::FromHost::ShowPerf(perf_data) => {
                        // Reschedule pending no-data timeout.
                        ctx.shared.timeout_handle.lock(|timeout_opt| {
                            timeout_opt.take().map(|timeout| timeout.cancel().ok());
                            *timeout_opt = no_data_timeout::spawn_after(2.secs(), false).ok();
                        });

                        handle_perf::spawn(perf_data).ok();
                    }
                    _ => {}
                }
            }
            Err(_) => {
                error!("Failed to deserialize message");
                asm::bkpt();
            }
        }
    }

    /// Displays PerfData smoothly, by averaging new_perf with prev_perf.  It then updates
    /// prev_perf, and schedules itself to display that value directly.
    #[task(priority = 2, shared = [prev_perf, frames])]
    fn handle_perf(ctx: handle_perf::Context, new_perf: PerfData) {
        let handle_perf::SharedResources { prev_perf, frames } = ctx.shared;

        (prev_perf, frames).lock(
            |prev_perf: &mut Option<PerfData>, frames: &mut FramesDeque| {
                let prev_value = prev_perf.take();

                // Calculate perf data to display, and previous data to keep.
                *prev_perf = perf::update_state(prev_value, new_perf, frames);
            },
        );
    }

    /// Immediately displays provided PerfData.
    #[task(shared = [display, frames])]
    fn show_perf(ctx: show_perf::Context) {
        let show_perf::SharedResources { display, frames } = ctx.shared;

        if let Err(_) = show_perf::spawn_at(monotonics::now() + perf::FRAME_MS.millis()) {
            error!("Failed to request show_perf::spawn_at");
            asm::bkpt();
        }

        // Pop a frame off the front of the frame queue and display it.
        (display, frames).lock(|display: &mut Display, frames: &mut FramesDeque| {
            if let Some(frame) = frames.pop_front() {
                gfx::draw_perf(display, &frame).unwrap();
                // if let Err(_) = display.flush() {
                //     error!("Failed to flush display");
                //     #[cfg(debug_assertions)]
                //     asm::bkpt();
                // }
            }
        });
    }

    #[task(priority = 2, shared = [display, timeout_handle])]
    fn no_data_timeout(ctx: no_data_timeout::Context, clear_screen: bool) {
        let no_data_timeout::SharedResources {
            mut display,
            mut timeout_handle,
        } = ctx.shared;

        display.lock(|display| {
            if clear_screen {
                warn!("No perf data received in {} seconds", BLANK_SCREEN_SECS);
                display.clear(Rgb565::BLACK).ok();
            } else {
                info!("No perf data received recently");
                gfx::draw_message(display, "No data received").ok();

                // Schedule clear screen timeout.
                timeout_handle.lock(|timeout_opt| {
                    timeout_opt.take().map(|timeout| timeout.cancel().ok());
                    *timeout_opt =
                        no_data_timeout::spawn_after(BLANK_SCREEN_SECS.secs(), true).ok();
                });
            }
        });
    }
}

/// Handles high and low priority USB interrupts.
fn handle_usb_event(serial: &mut io::Serial) {
    let mut result = [0u8; io::BUF_BYTES];
    let len = serial.read_packet(&mut result[..]).unwrap();
    if len > 0 {
        defmt::debug!("non-empty packet recvd");
        if let Err(_) = app::handle_packet::spawn(result) {
            error!("Failed to spawn handle_packet, likely still handling last packet")
        }
    }
}
