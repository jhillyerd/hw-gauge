#![no_main]
#![no_std]

use defmt::error;
use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::rp2040::prelude::*;

mod gfx;
mod io;
mod perf;

rp2040_timer_monotonic!(Mono);

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
#[link_section = ".boot2"]
#[no_mangle]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[rtic::app(
    device = rp2040_hal::pac,
    peripherals = true,
    dispatchers = [ PIO0_IRQ_0, PIO0_IRQ_1, PIO1_IRQ_0 ],
)]
mod app {
    use super::*;

    use crate::{
        gfx, io,
        perf::{self, FramesDeque, PerfFrame},
    };
    use core::mem::MaybeUninit;
    use cortex_m::asm;
    use defmt::{debug, error, expect, info, unwrap, warn};
    use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
    use embedded_graphics_framebuf::FrameBuf;
    use embedded_hal::{digital::OutputPin, spi};
    use fugit::{ExtU64, RateExtU32};
    use postcard;
    use rp2040_hal::{self as hal, clocks::Clock, gpio, usb, watchdog::Watchdog};
    use shared::{message, message::PerfData};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the board crystal.
    const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

    // Duration to illuminate status LED upon data RX.
    const STATUS_LED_MS: u64 = 50;

    // Delay from no data received to blanking the screen.
    const BLANK_SCREEN_MS: u64 = 30000;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    // LED blinks on USB activity.
    type ActivityLED =
        gpio::Pin<gpio::bank0::Gpio25, gpio::FunctionSio<gpio::SioOutput>, gpio::PullDown>;

    type DisplayBuf = FrameBuf<Rgb565, &'static mut [Rgb565; 240 * 135]>;

    // ST7789V IPS screen, aka T-Display.
    type Display = mipidsi::Display<
        display_interface_spi::SPIInterface<
            hal::Spi<
                hal::spi::Enabled,
                hal::pac::SPI0,
                (
                    gpio::Pin<gpio::bank0::Gpio3, gpio::FunctionSpi, gpio::PullDown>,
                    gpio::Pin<gpio::bank0::Gpio2, gpio::FunctionSpi, gpio::PullDown>,
                ),
                8,
            >,
            gpio::Pin<gpio::bank0::Gpio1, gpio::FunctionSio<gpio::SioOutput>, gpio::PullDown>,
            gpio::Pin<gpio::bank0::Gpio5, gpio::FunctionSio<gpio::SioOutput>, gpio::PullDown>,
        >,
        mipidsi::models::ST7789,
        gpio::Pin<gpio::bank0::Gpio0, gpio::FunctionSio<gpio::SioOutput>, gpio::PullDown>,
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

        // Last time we received a valid message.
        msg_time: <Mono as rtic_monotonics::Monotonic>::Instant,
    }

    #[local]
    struct Local {
        led: crate::app::ActivityLED,
        frame_buf: crate::app::DisplayBuf,
    }

    #[init(local = [
           usb_bus: Option<UsbBusAllocator<usb::UsbBus>> = None,
           frame_buf_store: MaybeUninit<[Rgb565; 240 * 135]> = MaybeUninit::uninit(),
    ])]
    fn init(ctx: init::Context) -> (Shared, Local) {
        // Soft-reset does not release the hardware spinlocks.
        // Release them now to avoid a deadlock after debug or watchdog reset.
        unsafe {
            hal::sio::spinlock_reset();
        }

        let mut resets = ctx.device.RESETS;

        info!("RTIC init started");

        // Setup clock & timer.
        Mono::start(ctx.device.TIMER, &resets);
        let mut watchdog = Watchdog::new(ctx.device.WATCHDOG);
        let clocks = unwrap!(hal::clocks::init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            ctx.device.XOSC,
            ctx.device.CLOCKS,
            ctx.device.PLL_SYS,
            ctx.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok());

        let mut delay =
            cortex_m::delay::Delay::new(ctx.core.SYST, clocks.system_clock.freq().to_Hz());

        // Setup status LED.
        let sio = hal::Sio::new(ctx.device.SIO);
        let pins = gpio::Pins::new(
            ctx.device.IO_BANK0,
            ctx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );
        let mut led = pins.gpio25.into_push_pull_output();
        unwrap!(led.set_low());

        // Init frame buffer.
        let frame_buf_store: &'static mut _ =
            ctx.local.frame_buf_store.write([Rgb565::BLACK; 240 * 135]);
        let frame_buf: DisplayBuf = FrameBuf::new(&mut *frame_buf_store, 240, 135);

        // Setup SPI bus for onboard "T-Display".
        let spi_sclk = pins.gpio2.into_function::<gpio::FunctionSpi>();
        let spi_mosi = pins.gpio3.into_function::<gpio::FunctionSpi>();
        let spi = hal::Spi::<_, _, _, 8>::new(ctx.device.SPI0, (spi_mosi, spi_sclk)).init(
            &mut resets,
            clocks.peripheral_clock.freq(),
            15.MHz(), // 66ns minimum clock cycle time for ST7789V.
            spi::MODE_3,
        );

        // Setup T-Display.
        // TODO: Investigate PWM for night time.
        unwrap!(pins.gpio22.into_push_pull_output().set_high()); // Power on display.
        let mut bl_pin = pins.gpio4.into_push_pull_output();
        unwrap!(bl_pin.set_low()); // Backlight off until we've cleared the display.

        let cs_pin = pins.gpio5.into_push_pull_output();
        let dc_pin = pins.gpio1.into_push_pull_output();
        let rst_pin = pins.gpio0.into_push_pull_output();
        let display_if = display_interface_spi::SPIInterface::new(spi, dc_pin, cs_pin);
        let mut display = expect!(
            mipidsi::builder::Builder::st7789_pico1(display_if)
                .with_orientation(mipidsi::options::Orientation::Landscape(true))
                .init(&mut delay, Some(rst_pin)),
            "display initializes"
        );

        expect!(display.clear(Rgb565::BLACK), "display clears");
        unwrap!(bl_pin.set_high());

        // Setup USB bus and serial port device.
        *ctx.local.usb_bus = Some(UsbBusAllocator::new(usb::UsbBus::new(
            ctx.device.USBCTRL_REGS,
            ctx.device.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut resets,
        )));
        let port = usbd_serial::SerialPort::new(unwrap!(ctx.local.usb_bus.as_ref()));
        let usb_dev = expect!(
            UsbDeviceBuilder::new(
                unwrap!(ctx.local.usb_bus.as_ref()),
                UsbVidPid(USB_VENDOR_ID, USB_PRODUCT_ID),
            )
            .strings(&[
                StringDescriptors::new(LangID::EN).manufacturer("JHillyerd"),
                StringDescriptors::new(LangID::EN).product("System monitor"),
            ]),
            "Failed to set usb_device strings"
        );
        let usb_dev = usb_dev.device_class(usbd_serial::USB_CLASS_CDC).build();

        // Start tasks.
        unwrap!(pulse_led::spawn());
        unwrap!(show_perf::spawn());
        unwrap!(no_data_timeout::spawn());

        info!("RTIC init completed");

        (
            Shared {
                frames: FramesDeque::new(),
                serial: io::Serial::new(usb_dev, port),
                display,
                pulse_led: false,
                prev_perf: None,
                msg_time: Mono::now(),
            },
            Local { led, frame_buf },
        )
    }

    #[task(shared = [pulse_led], local = [led])]
    async fn pulse_led(ctx: pulse_led::Context) {
        let mut pulse_led = ctx.shared.pulse_led;
        let led = ctx.local.led;

        loop {
            pulse_led.lock(|pulse_led| {
                if *pulse_led {
                    led.set_high().unwrap();
                    *pulse_led = false;
                } else {
                    led.set_low().unwrap();
                }
            });

            // Clear LED after a delay.
            Mono::delay(STATUS_LED_MS.millis()).await;
        }
    }

    #[task(priority = 4, binds = USBCTRL_IRQ, shared = [serial, pulse_led])]
    fn usb_event(ctx: usb_event::Context) {
        // TODO: schedule 10ms poll to be compliant.
        let usb_event::SharedResources {
            serial, pulse_led, ..
        } = ctx.shared;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task(priority = 3, shared = [msg_time])]
    async fn handle_packet(mut ctx: handle_packet::Context, mut buf: [u8; io::BUF_BYTES]) {
        let msg: Result<message::FromHost, _> = postcard::from_bytes_cobs(&mut buf);
        match msg {
            Ok(msg) => {
                debug!("Rx message: {:?}", msg);
                if let message::FromHost::ShowPerf(perf_data) = msg {
                    ctx.shared.msg_time.lock(|msg_time| {
                        *msg_time = Mono::now();
                    });

                    // TODO: should use a queue here.
                    handle_perf::spawn(perf_data).ok();
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
    async fn handle_perf(ctx: handle_perf::Context, new_perf: PerfData) {
        let handle_perf::SharedResources {
            prev_perf, frames, ..
        } = ctx.shared;

        (prev_perf, frames).lock(
            |prev_perf: &mut Option<PerfData>, frames: &mut FramesDeque| {
                let prev_value = prev_perf.take();

                // Calculate perf data to display, and previous data to keep.
                *prev_perf = perf::update_state(prev_value, new_perf, frames);
            },
        );
    }

    /// Loop which displays available perf frames.
    #[task(shared = [display, frames], local = [frame_buf])]
    async fn show_perf(ctx: show_perf::Context) -> ! {
        let show_perf::SharedResources {
            mut display,
            mut frames,
            ..
        } = ctx.shared;
        let frame_buf = ctx.local.frame_buf;
        let mut instant = Mono::now();

        loop {
            // Use absolute delay to prevent drift.
            instant += perf::FRAME_MS.millis();
            Mono::delay_until(instant).await;

            // Pop a frame off the front of the frame queue and display it.
            (&mut display, &mut frames).lock(|display: &mut Display, frames: &mut FramesDeque| {
                match frames.pop_front() {
                    Some(PerfFrame::Complete(frame)) => {
                        gfx::draw_perf(frame_buf, &frame).unwrap();
                        display.draw_iter(frame_buf.into_iter()).unwrap();
                    }
                    Some(PerfFrame::Partial(frame)) => {
                        gfx::draw_cpu_bar_graph(display, &frame).unwrap();
                    }
                    None => {}
                }
            });
        }
    }

    #[task(priority = 2, shared = [display, msg_time])]
    async fn no_data_timeout(ctx: no_data_timeout::Context) -> ! {
        let no_data_timeout::SharedResources {
            mut display,
            mut msg_time,
            ..
        } = ctx.shared;

        #[derive(PartialEq)]
        enum TimeoutState {
            None,
            NoData,
            ClearScreen,
        }
        let mut state = TimeoutState::None;

        loop {
            Mono::delay(250.millis()).await;
            let instant = Mono::now();

            msg_time.lock(|msg_time| {
                let elapsed = match instant.checked_duration_since(*msg_time) {
                    Some(elapsed) => elapsed,
                    None => return,
                };

                if elapsed.to_millis() < 2000 {
                    state = TimeoutState::None;
                    return;
                }

                display.lock(|display| {
                    if elapsed.to_millis() < BLANK_SCREEN_MS {
                        if state != TimeoutState::NoData {
                            state = TimeoutState::NoData;
                            info!("No perf data received recently");
                            gfx::draw_message(display, "No data received").ok();
                        }
                    } else if state != TimeoutState::ClearScreen {
                        state = TimeoutState::ClearScreen;
                        // TODO disable backlight
                        warn!("No perf data received in {} ms", BLANK_SCREEN_MS);
                        display.clear(Rgb565::BLACK).ok();
                    }
                });
            });
        }
    }
}

/// Handles high and low priority USB interrupts.
fn handle_usb_event(serial: &mut io::Serial) {
    let mut result = [0u8; io::BUF_BYTES];
    let len = serial.read_packet(&mut result[..]).unwrap();
    if len > 0 && app::handle_packet::spawn(result).is_err() {
        error!("Failed to spawn handle_packet, likely still handling last packet")
    }
}
