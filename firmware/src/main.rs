#![no_main]
#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};
use defmt_rtt as _;
use panic_probe as _;

mod gfx;
mod io;

#[rtic::app(
    device = stm32f1xx_hal::pac,
    dispatchers = [SPI1, SPI2]
)]
mod app {
    use crate::{gfx, io};
    use cortex_m::asm;
    use defmt::{assert, debug, error, info, warn};
    use dwt_systick_monotonic::DwtSystick;
    use embedded_hal::digital::v2::*;
    use postcard;
    use shared::{message, message::PerfData};
    use ssd1306::prelude::*;
    use stm32f1xx_hal::{gpio::*, i2c, pac, prelude::*, rcc::Clocks, timer, usb};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the system clock, which will also be the frequency of CYCCNT.
    const SYSCLK_HZ: u32 = 72_000_000;

    // Frequency of timer used for updating display, checking received perf timeout.
    const TIMER_HZ: u32 = 10;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const USB_RESET_PERIOD: u32 = SYSCLK_HZ / 100;
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    #[monotonic(binds = SysTick, default = true)]
    type SysMono = DwtSystick<{ crate::app::SYSCLK_HZ }>;

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

    #[shared]
    struct Shared {
        led: crate::app::ActivityLED,
        serial: io::Serial,
        display: crate::app::Display,

        // Blinks ActivityLED briefly when set true.
        pulse_led: bool,

        // Previously received perf data message.
        prev_perf: Option<PerfData>,

        // Milliseconds since device last received a perf packet over USB.
        prev_perf_ms: u32,
    }

    #[local]
    struct Local {
        timer: timer::CountDownTimer<pac::TIM2>,
    }

    #[init(local = [usb_bus: Option<UsbBusAllocator<usb::UsbBusType>> = None])]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        info!("RTIC 0.6.0-rc.4 init started");
        let mut cp: cortex_m::Peripherals = ctx.core;
        let dp: pac::Peripherals = ctx.device;

        // Setup and apply clock confiugration.
        let mut flash = dp.FLASH.constrain();
        let mut rcc = dp.RCC.constrain();
        let clocks: Clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(SYSCLK_HZ.hz())
            .pclk1((SYSCLK_HZ / 2).hz())
            .freeze(&mut flash.acr);
        let mono = DwtSystick::new(&mut cp.DCB, cp.DWT, cp.SYST, clocks.sysclk().0);
        assert!(clocks.usbclk_valid());

        // Countdown timer setup.
        let mut timer =
            timer::Timer::tim2(dp.TIM2, &clocks, &mut rcc.apb1).start_count_down(TIMER_HZ.hz());
        timer.listen(timer::Event::Update);

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

        (
            Shared {
                led,
                serial: io::Serial::new(usb_dev, port),
                display,
                pulse_led: false,
                prev_perf: None,
                prev_perf_ms: 0,
            },
            Local { timer },
            init::Monotonics(mono),
        )
    }

    #[task(priority = 1, binds = TIM2, shared = [prev_perf_ms, display], local = [timer])]
    fn tick(ctx: tick::Context) {
        let tick::SharedResources {
            mut prev_perf_ms,
            mut display,
        } = ctx.shared;

        ctx.local.timer.clear_update_interrupt_flag();

        prev_perf_ms.lock(|prev_perf_ms| {
            *prev_perf_ms += 1000 / TIMER_HZ;

            // Intervals below must divide evenly into the timer period.
            match *prev_perf_ms {
                500 => {
                    show_perf::spawn(None).ok();
                }
                2_000 => {
                    info!("No perf received in 2 seconds");
                    display.lock(|display| {
                        gfx::draw_message(display, "No data received").ok();
                        display.flush().ok();
                    });
                }
                30_000 => {
                    warn!("No perf received in 30 seconds");
                    display.lock(|display| {
                        display.clear();
                        display.flush().ok();
                    });
                }
                _ => {}
            }
        });

        pulse_led::spawn().ok();
    }

    #[task(shared = [led, pulse_led])]
    fn pulse_led(ctx: pulse_led::Context) {
        let pulse_led::SharedResources { led, pulse_led } = ctx.shared;

        (led, pulse_led).lock(|led: &mut ActivityLED, pulse_led| {
            if *pulse_led {
                led.set_low().ok();
                *pulse_led = false;
            } else {
                led.set_high().ok();
            }
        });
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

    #[task(shared = [prev_perf_ms])]
    fn handle_packet(ctx: handle_packet::Context, mut buf: [u8; io::BUF_BYTES]) {
        let handle_packet::SharedResources { mut prev_perf_ms } = ctx.shared;

        let msg: Result<message::FromHost, _> = postcard::from_bytes_cobs(&mut buf);
        match msg {
            Ok(msg) => {
                debug!("Rx message: {:?}", msg);
                match msg {
                    message::FromHost::ShowPerf(perf_data) => {
                        prev_perf_ms.lock(|ticks| *ticks = 0);
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
    #[task(shared = [prev_perf, display])]
    fn show_perf(ctx: show_perf::Context, new_perf: Option<PerfData>) {
        let show_perf::SharedResources { prev_perf, display } = ctx.shared;

        (prev_perf, display).lock(|prev_perf: &mut Option<PerfData>, display: &mut Display| {
            let prev_value = prev_perf.take();
            let perf_data: Option<PerfData> = match (prev_value, new_perf) {
                (Some(prev), None) => {
                    // Display previous perf packet unaltered.
                    *prev_perf = Some(prev);
                    Some(prev)
                }
                (None, Some(new)) => {
                    // Display new perf packet unaltered, as there is no history.
                    *prev_perf = Some(new);
                    Some(new)
                }
                (Some(prev), Some(new)) => {
                    // Display average of new and previous perf packets.
                    *prev_perf = Some(new);
                    Some(PerfData {
                        all_cores_load: (prev.all_cores_load + new.all_cores_load) / 2.0,
                        all_cores_avg: new.all_cores_avg,
                        peak_core_load: (prev.peak_core_load + new.peak_core_load) / 2.0,
                        memory_load: new.memory_load,
                        daytime: new.daytime,
                    })
                }
                _ => {
                    // This is expected during startup.
                    None
                }
            };

            debug!("Will display: {:?}", perf_data);

            if let Some(perf_data) = perf_data {
                gfx::draw_perf(display, &perf_data).unwrap();
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

static COUNT: AtomicUsize = AtomicUsize::new(0);
defmt::timestamp!("{=usize}", {
    // NOTE(no-CAS) `timestamps` runs with interrupts disabled
    let n = COUNT.load(Ordering::Relaxed);
    COUNT.store(n + 1, Ordering::Relaxed);
    n
});
