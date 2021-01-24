#![no_main]
#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};
use defmt_rtt as _;
use panic_probe as _;

mod gfx;
mod io;

#[rtic::app(
    device = stm32f1xx_hal::pac,
    peripherals = true,
    monotonic = rtic::cyccnt::CYCCNT,
    dispatchers = [SPI1, SPI2]
)]
mod app {
    use crate::{gfx, io};
    use cortex_m::asm;
    use defmt::{assert, debug, error, info};
    use embedded_hal::digital::v2::*;
    use postcard;
    use rtic::cyccnt::U32Ext;
    use rtic_core::prelude::*;
    use shared::{message, message::PerfData};
    use ssd1306::prelude::*;
    use stm32f1xx_hal::{gpio::*, i2c, pac, prelude::*, rcc::Clocks, timer, usb};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the system clock, which will also be the frequency of CYCCNT.
    const SYSCLK_HZ: u32 = 72_000_000;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const PULSE_LED_PERIOD: u32 = SYSCLK_HZ / 10;
    const USB_RESET_PERIOD: u32 = SYSCLK_HZ / 100;
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    // LED blinks on USB activity.
    type ActivityLED = gpioc::PC13<Output<PushPull>>;

    // 128x64 OLED I2C display.
    type Display = ssd1306::mode::GraphicsMode<
        I2CInterface<
            i2c::BlockingI2c<
                pac::I2C2,
                (
                    gpiob::PB10<Alternate<OpenDrain>>,
                    gpiob::PB11<Alternate<OpenDrain>>,
                ),
            >,
        >,
        DisplaySize128x64,
    >;

    #[resources]
    struct Resources {
        led: ActivityLED,
        serial: io::Serial,
        display: Display,

        // Blinks ActivityLED briefly when set true.
        #[init(false)]
        pulse_led: bool,

        // Previously received perf data message.
        #[init(None)]
        prev_perf: Option<PerfData>,
    }

    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<UsbBusAllocator<usb::UsbBusType>> = None;

        info!("RTIC 0.6 init started");
        let mut cp = ctx.core;
        let dp: pac::Peripherals = ctx.device;

        // Enable CYCCNT; used for scheduling.
        cp.DWT.enable_cycle_counter();

        // Setup and apply clock confiugration.
        let mut flash = dp.FLASH.constrain();
        let mut rcc = dp.RCC.constrain();
        let clocks: Clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(SYSCLK_HZ.hz())
            .pclk1((SYSCLK_HZ / 2).hz())
            .freeze(&mut flash.acr);
        assert!(clocks.usbclk_valid());

        // Countdown timer setup.
        let mut scope_timer =
            timer::Timer::tim2(dp.TIM2, &clocks, &mut rcc.apb1).start_count_down(2.khz());
        scope_timer.listen(timer::Event::Update);

        // Peripheral setup.
        let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);
        let mut gpioc = dp.GPIOC.split(&mut rcc.apb2);

        // USB serial setup.
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap(); // Reset USB bus at startup.
        asm::delay(USB_RESET_PERIOD);
        let usb_p = usb::Peripheral {
            usb: dp.USB,
            pin_dm: gpioa.pa11,
            pin_dp: usb_dp.into_floating_input(&mut gpioa.crh),
        };
        *USB_BUS = Some(usb::UsbBus::new(usb_p));
        let port = usbd_serial::SerialPort::new(USB_BUS.as_ref().unwrap());
        let usb_dev = UsbDeviceBuilder::new(
            USB_BUS.as_ref().unwrap(),
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
            i2c::Mode::fast(400_000.hz(), i2c::DutyCycle::Ratio2to1),
            clocks,
            &mut rcc.apb1,
            1000,
            10,
            1000,
            1000,
        );

        // Display setup.
        let disp_if = ssd1306::I2CDIBuilder::new().init(i2c2);
        let mut display: GraphicsMode<_, _> = ssd1306::Builder::new().connect(disp_if).into();
        display.init().unwrap();
        display.clear();
        display.flush().unwrap();

        // Configure pc13 as output via CR high register.
        let mut led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);
        led.set_high().unwrap(); // LED off

        // Start scheduled tasks.
        // TODO: Switch to spawn after https://github.com/rtic-rs/cortex-m-rtic/issues/403
        pulse_led::schedule(ctx.start).unwrap();

        // Prevent wait-for-interrupt (default rtic idle) from stalling debug features.
        //
        // See: https://github.com/probe-rs/probe-rs/issues/350
        dp.DBGMCU.cr.modify(|_, w| {
            w.dbg_sleep().set_bit();
            w.dbg_standby().set_bit();
            w.dbg_stop().set_bit()
        });
        let _dma1 = dp.DMA1.split(&mut rcc.ahb);

        info!("RTIC init completed");

        init::LateResources {
            led,
            serial: io::Serial::new(usb_dev, port),
            display,
        }
    }

    #[task(resources = [led, pulse_led])]
    fn pulse_led(ctx: pulse_led::Context) {
        let pulse_led::Resources { led, pulse_led } = ctx.resources;

        (led, pulse_led).lock(|led: &mut ActivityLED, pulse_led| {
            if *pulse_led {
                led.set_low().ok();
                *pulse_led = false;
            } else {
                led.set_high().ok();
            }
        });

        pulse_led::schedule(ctx.scheduled + PULSE_LED_PERIOD.cycles()).unwrap();
    }

    #[task(binds = USB_HP_CAN_TX, resources = [serial, pulse_led])]
    fn usb_high(ctx: usb_high::Context) {
        let usb_high::Resources { serial, pulse_led } = ctx.resources;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task(binds = USB_LP_CAN_RX0, resources = [serial, pulse_led])]
    fn usb_low(ctx: usb_low::Context) {
        let usb_low::Resources { serial, pulse_led } = ctx.resources;
        (serial, pulse_led).lock(|serial, pulse_led| {
            crate::handle_usb_event(serial);
            *pulse_led = true;
        });
    }

    #[task]
    fn handle_packet(_ctx: handle_packet::Context, mut buf: [u8; io::BUF_BYTES]) {
        let msg: Result<message::FromHost, _> = postcard::from_bytes_cobs(&mut buf);
        match msg {
            Ok(msg) => {
                debug!("Rx message: {:?}", msg);
                match msg {
                    message::FromHost::ShowPerf(perf_data) => {
                        show_perf::spawn(Some(perf_data)).ok();
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
    #[task(resources = [prev_perf, display])]
    fn show_perf(ctx: show_perf::Context, new_perf: Option<PerfData>) {
        let show_perf::Resources { prev_perf, display } = ctx.resources;

        if let Some(_) = new_perf {
            // Schedule a follow-up show_perf that will show unaltered prev_perf values.
            show_perf::spawn(None).ok();
        } else {
            // This is the follow-up schedule above, wait for a bit.
            // TODO: Replace this with a normal schedule when RTIC monotonic is fixed.
            asm::delay(SYSCLK_HZ / 2);
        }

        (prev_perf, display).lock(|prev_perf: &mut Option<PerfData>, display: &mut Display| {
            let prev_value = prev_perf.take();
            let perf_data: Option<PerfData> = match (prev_value, new_perf) {
                (Some(prev), None) => {
                    *prev_perf = Some(prev);
                    Some(prev)
                }
                (None, Some(new)) => {
                    *prev_perf = Some(new);
                    Some(new)
                }
                (Some(prev), Some(new)) => {
                    *prev_perf = Some(new);
                    Some(PerfData {
                        all_cores_load: (prev.all_cores_load + new.all_cores_load) / 2.0,
                        peak_core_load: (prev.peak_core_load + new.peak_core_load) / 2.0,
                    })
                }
                _ => {
                    error!("No new or previous PerfData");
                    None
                }
            };

            debug!("Will display: {:?}", perf_data);

            if let Some(perf_data) = perf_data {
                gfx::draw(display, &perf_data).unwrap();
                if let Err(_) = display.flush() {
                    error!("Failed to flush display");
                    #[cfg(debug_assertions)]
                    asm::bkpt();
                }
            }
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

#[defmt::timestamp]
fn timestamp() -> u64 {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    // NOTE(no-CAS) `timestamps` runs with interrupts disabled
    let n = COUNT.load(Ordering::Relaxed);
    COUNT.store(n + 1, Ordering::Relaxed);
    n as u64
}
