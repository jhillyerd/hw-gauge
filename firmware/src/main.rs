#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

mod gfx;
mod io;
mod mono;
mod perf;

#[rtic::app(
    device = stm32f1xx_hal::pac,
    dispatchers = [SPI1, SPI2]
)]
mod app {
    use crate::{gfx, io, mono::*, perf};
    use cortex_m::asm;
    use defmt::{assert, debug, error, info, warn};
    use fugit::RateExtU32;
    use postcard;
    use shared::{message, message::PerfData};
    use ssd1306::{prelude::*, rotation::DisplayRotation, size::DisplaySize128x64, Ssd1306};
    use stm32f1xx_hal::{gpio::*, i2c, pac, prelude::*, rcc::Clocks, usb};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the system clock, which will also be the frequency of CYCCNT.
    const SYSCLK_HZ: u32 = 72_000_000;

    // Duration to illuninate status LED upon data RX.
    const STATUS_LED_MS: u32 = 50;

    // Delay from no data received to blanking the screen.
    const BLANK_SCREEN_SECS: u32 = 30;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const USB_RESET_PERIOD: u32 = SYSCLK_HZ / 100;
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    #[monotonic(binds = TIM2, default = true)]
    type SysMono = MonoTimer<pac::TIM2, 2000>;

    // LED blinks on USB activity.
    type ActivityLED = gpioc::PC13<Output<PushPull>>;

    // 128x64 OLED I2C display.
    type Display = Ssd1306<
        I2CInterface<
            i2c::BlockingI2c<
                pac::I2C2,
                (
                    Pin<Alternate<OpenDrain>, CRH, 'B', 10>,
                    Pin<Alternate<OpenDrain>, CRH, 'B', 11>,
                ),
            >,
        >,
        DisplaySize128x64,
        ssd1306::mode::BufferedGraphicsMode<DisplaySize128x64>,
    >;

    #[shared]
    struct Shared {
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

    #[init(local = [usb_bus: Option<UsbBusAllocator<usb::UsbBusType>> = None])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("RTIC init started");
        let dp: pac::Peripherals = ctx.device;

        // Setup and apply clock confiugration.
        let mut flash = dp.FLASH.constrain();
        let rcc = dp.RCC.constrain();
        let clocks: Clocks = rcc
            .cfgr
            .use_hse(8.MHz())
            .sysclk(SYSCLK_HZ.Hz())
            .pclk1((SYSCLK_HZ / 2).Hz())
            .freeze(&mut flash.acr);
        let mono = SysMono::new(dp.TIM2, &clocks);
        assert!(clocks.usbclk_valid());

        // Peripheral setup.
        let mut gpioa = dp.GPIOA.split();
        let mut gpiob = dp.GPIOB.split();
        let mut gpioc = dp.GPIOC.split();

        // USB serial setup.
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low(); // Reset USB bus at startup.
        asm::delay(USB_RESET_PERIOD);
        let usb_p = usb::Peripheral {
            usb: dp.USB,
            pin_dm: gpioa.pa11,
            pin_dp: usb_dp.into_floating_input(&mut gpioa.crh),
        };
        *ctx.local.usb_bus = Some(usb::UsbBus::new(usb_p));
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

        // I2C setup.
        let scl = gpiob.pb10.into_alternate_open_drain(&mut gpiob.crh);
        let sda = gpiob.pb11.into_alternate_open_drain(&mut gpiob.crh);
        let i2c2 = i2c::BlockingI2c::i2c2(
            dp.I2C2,
            (scl, sda),
            i2c::Mode::fast(400_000.Hz(), i2c::DutyCycle::Ratio2to1),
            clocks,
            1000,
            10,
            1000,
            1000,
        );

        // Display setup.
        let disp_if = ssd1306::I2CDisplayInterface::new(i2c2);
        let mut display = Ssd1306::new(disp_if, DisplaySize128x64, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();
        display.init().unwrap();
        display.clear();
        display.flush().unwrap();

        // Configure pc13 (status LED) as output via CR high register.
        let mut led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);
        led.set_high(); // LED off

        // Prevent wait-for-interrupt (default rtic idle) from stalling debug features.
        //
        // See: https://github.com/probe-rs/probe-rs/issues/350
        dp.DBGMCU.cr.modify(|_, w| {
            w.dbg_sleep().set_bit();
            w.dbg_standby().set_bit();
            w.dbg_stop().set_bit()
        });
        let _dma1 = dp.DMA1.split();

        // Start tasks.
        pulse_led::spawn().unwrap();

        info!("RTIC init completed");

        (
            Shared {
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

    #[task(shared = [pulse_led], local = [led])]
    fn pulse_led(ctx: pulse_led::Context) {
        let mut pulse_led = ctx.shared.pulse_led;
        let led = ctx.local.led;

        pulse_led.lock(|pulse_led| {
            if *pulse_led {
                led.set_low();
                *pulse_led = false;
            } else {
                led.set_high();
            }
        });

        // Clear LED after a delay.
        pulse_led::spawn_after(STATUS_LED_MS.millis()).unwrap();
    }

    #[task(priority = 2, binds = USB_HP_CAN_TX, shared = [serial, pulse_led])]
    fn usb_high(ctx: usb_high::Context) {
        let usb_high::SharedResources { serial, pulse_led } = ctx.shared;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task(priority = 2, binds = USB_LP_CAN_RX0, shared = [serial, pulse_led])]
    fn usb_low(ctx: usb_low::Context) {
        let usb_low::SharedResources { serial, pulse_led } = ctx.shared;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task(shared = [timeout_handle])]
    fn handle_packet(mut ctx: handle_packet::Context, mut buf: [u8; io::BUF_BYTES]) {
        let msg: Result<message::FromHost, _> = postcard::from_bytes_cobs(&mut buf);
        match msg {
            Ok(msg) => {
                info!("Rx message: {:?}", msg);
                match msg {
                    message::FromHost::ShowPerf(perf_data) => {
                        // Reschedule pending no-data timeout.
                        ctx.shared.timeout_handle.lock(|timeout_opt| {
                            timeout_opt.take().map(|timeout| timeout.cancel().ok());
                            *timeout_opt = no_data_timeout::spawn_after(2.secs(), false).ok();
                        });

                        handle_perf::spawn(Some(perf_data)).ok();
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
    #[task(shared = [prev_perf])]
    fn handle_perf(mut ctx: handle_perf::Context, new_perf: Option<PerfData>) {
        ctx.shared
            .prev_perf
            .lock(|prev_perf: &mut Option<PerfData>| {
                let prev_value = prev_perf.take();

                // Calculate perf data to display, and previous data to keep.
                let mut state = perf::State {
                    previous: prev_value,
                    // TODO: current is useless
                    current: new_perf,
                };
                state = perf::update_state(state);
                *prev_perf = state.previous;
            });
    }

    /// Immediately displays provided PerfData.
    #[task(capacity = 30, shared = [display])]
    fn show_perf(mut ctx: show_perf::Context, perf: PerfData) {
        debug!("Displ CPU PEAK: {}", perf.peak_core_load);

        ctx.shared.display.lock(|display: &mut Display| {
            gfx::draw_perf(display, &perf).unwrap();
            if let Err(_) = display.flush() {
                error!("Failed to flush display");
                #[cfg(debug_assertions)]
                asm::bkpt();
            }
        });
    }

    #[task(shared = [display, timeout_handle])]
    fn no_data_timeout(ctx: no_data_timeout::Context, clear_screen: bool) {
        let no_data_timeout::SharedResources {
            mut display,
            mut timeout_handle,
        } = ctx.shared;

        display.lock(|display| {
            if clear_screen {
                warn!("No perf data received in {} seconds", BLANK_SCREEN_SECS);
                display.clear();
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

            display.flush().ok();
        });
    }
}

/// Handles high and low priority USB interrupts.
fn handle_usb_event(serial: &mut io::Serial) {
    let mut result = [0u8; io::BUF_BYTES];
    let len = serial.read_packet(&mut result[..]).unwrap();
    if len > 0 {
        app::handle_packet::spawn(result).unwrap();
    }
}
