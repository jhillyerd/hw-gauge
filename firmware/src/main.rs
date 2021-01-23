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
    use crate::io;
    use crate::Direction;
    use cortex_m::asm::delay;
    use defmt::{error, info};
    use embedded_hal::digital::v2::*;
    use postcard;
    use rtic::cyccnt::U32Ext;
    use rtic_core::prelude::*;
    use shared::message;
    use stm32f1xx_hal::{gpio::*, pac, prelude::*, pwm, rcc::Clocks, timer, usb};
    use usb_device::{bus::UsbBusAllocator, prelude::*};

    // Frequency of the system clock, which will also be the frequency of CYCCNT.
    const SYSCLK_HZ: u32 = 72_000_000;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const PULSE_LED_PERIOD: u32 = SYSCLK_HZ / 40;
    const USB_RESET_PERIOD: u32 = SYSCLK_HZ / 100;
    const USB_VENDOR_ID: u16 = 0x1209; // pid.codes VID.
    const USB_PRODUCT_ID: u16 = 0x0001; // In house private testing only.

    // Levels for offboard PWM LED blink.
    const PWM_LEVELS: [u16; 8] = [0, 5, 10, 15, 25, 40, 65, 100];
    type PwmLED = gpioa::PA6<Alternate<PushPull>>;
    type ActivityLED = gpioc::PC13<Output<PushPull>>;

    #[resources]
    struct Resources {
        #[init(0)]
        pwm_level: usize, // Index into PWM_LEVELS.
        led: ActivityLED,
        #[init(false)]
        pulse_led: bool,
        scope: gpioa::PA4<Output<PushPull>>,
        scope_timer: timer::CountDownTimer<pac::TIM2>,
        led_pwm: pwm::Pwm<pac::TIM3, timer::Tim3NoRemap, pwm::C1, PwmLED>,
        serial: io::Serial,
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
        defmt::assert!(clocks.usbclk_valid());

        // Countdown timer setup.
        let mut scope_timer =
            timer::Timer::tim2(dp.TIM2, &clocks, &mut rcc.apb1).start_count_down(2.khz());
        scope_timer.listen(timer::Event::Update);

        // Peripheral setup.
        let mut afio = dp.AFIO.constrain(&mut rcc.apb2);
        let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
        let mut gpioc = dp.GPIOC.split(&mut rcc.apb2);

        // USB serial setup.
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().unwrap(); // Reset USB bus at startup.
        delay(USB_RESET_PERIOD);
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

        // Configure pc13 as output via CR high register.
        let mut led = gpioc.pc13.into_push_pull_output(&mut gpioc.crh);
        led.set_high().unwrap(); // LED off

        // Configure pa4 as output for oscilloscope.
        let mut scope = gpioa.pa4.into_push_pull_output(&mut gpioa.crl);
        scope.set_low().unwrap(); // Oscill low

        // Setup TIM3 PWM CH1 on PA6.
        let pa6 = gpioa.pa6.into_alternate_push_pull(&mut gpioa.crl);
        let pwm_pins = pa6;
        let mut led_pwm = timer::Timer::tim3(dp.TIM3, &clocks, &mut rcc.apb1).pwm(
            pwm_pins,
            &mut afio.mapr,
            1.khz(),
        );
        led_pwm.set_duty(pwm::Channel::C1, 0);
        led_pwm.enable(pwm::Channel::C1);

        // Start scheduled tasks.
        // TODO: switch to spawn after https://github.com/rtic-rs/cortex-m-rtic/issues/403
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
            scope,
            scope_timer,
            led_pwm,
            serial: io::Serial::new(usb_dev, port),
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

    #[task(binds = TIM2, priority = 3, resources = [scope, scope_timer])]
    fn toggle_scope(ctx: toggle_scope::Context) {
        let toggle_scope::Resources { scope, scope_timer } = ctx.resources;

        (scope, scope_timer).lock(|scope, scope_timer| {
            scope.toggle().unwrap();
            scope_timer.clear_update_interrupt_flag();
        });
    }

    #[task(capacity = 4, resources = [pwm_level, led_pwm])]
    fn update_led_pwm(ctx: update_led_pwm::Context, dir: Direction) {
        let update_led_pwm::Resources { pwm_level, led_pwm } = ctx.resources;

        (pwm_level, led_pwm).lock(|pwm_level, led_pwm| {
            // Rotate pwm_level.
            *pwm_level = match dir {
                Direction::Up => (*pwm_level + 1) % PWM_LEVELS.len(),
                Direction::Down => {
                    if *pwm_level == 0 {
                        PWM_LEVELS.len() - 1
                    } else {
                        *pwm_level - 1
                    }
                }
            };

            // Set duty cycle.
            let max_duty = led_pwm.get_max_duty();
            let duty = max_duty / 100 * PWM_LEVELS[*pwm_level];
            led_pwm.set_duty(pwm::Channel::C1, duty);
            info!(
                "led_pwm duty = {:?}% ({:?} / {:?})",
                PWM_LEVELS[*pwm_level], duty, max_duty
            );
        });
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
            Ok(msg) => info!("got message: {:?}", msg),
            Err(_) => {
                error!("failed to deserialize message");
                cortex_m::asm::bkpt();
            }
        }
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

#[derive(Debug)]
pub enum Direction {
    Down,
    Up,
}

#[defmt::timestamp]
fn timestamp() -> u64 {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    // NOTE(no-CAS) `timestamps` runs with interrupts disabled
    let n = COUNT.load(Ordering::Relaxed);
    COUNT.store(n + 1, Ordering::Relaxed);
    n as u64
}
