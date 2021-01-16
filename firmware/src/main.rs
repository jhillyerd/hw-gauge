#![no_main]
#![no_std]

use rtt_target::rprintln;

#[rtic::app(
    device = stm32f1xx_hal::pac,
    peripherals = true,
    monotonic = rtic::cyccnt::CYCCNT,
    dispatchers = [SPI1, SPI2]
)]
mod app {
    use crate::Direction;
    use embedded_hal::digital::v2::*;
    use rtic::cyccnt::U32Ext;
    use rtic_core::prelude::*;
    use rtt_target::rprintln;
    use stm32f1xx_hal::{gpio::*, pac, prelude::*, pwm, rcc::Clocks, timer};

    // Frequency of the system clock, which will also be the frequency of CYCCNT.
    const SYSCLK_HZ: u32 = 72_000_000;

    // Periods are measured in system clock cycles; smaller is more frequent.
    const RTT_POLL_PERIOD: u32 = SYSCLK_HZ / 5;

    // Levels for offboard PWM LED blink.
    const PWM_LEVELS: [u16; 8] = [0, 5, 10, 15, 25, 40, 65, 100];
    type PwmLED = gpioa::PA6<Alternate<PushPull>>;

    #[resources]
    struct Resources {
        #[init(0)]
        pwm_level: usize, // Index into PWM_LEVELS.
        led: gpioc::PC13<Output<PushPull>>,
        scope: gpioa::PA4<Output<PushPull>>,
        scope_timer: timer::CountDownTimer<pac::TIM2>,
        led_pwm: pwm::Pwm<pac::TIM3, timer::Tim3NoRemap, pwm::C1, PwmLED>,
        // Used to read input from host over RTT.
        rtt_down: rtt_target::DownChannel,
    }

    #[init]
    fn init(ctx: init::Context) -> init::LateResources {
        // Initialize RTT communication with host.
        let rtt_channels = rtt_target::rtt_init_default!();
        rtt_target::set_print_channel(rtt_channels.up.0);

        rprintln!("RTIC 0.6 init started");
        let mut cp = ctx.core;
        let dp = ctx.device;

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
        rprintln!(" SYSCLK: {:?} MHz", clocks.sysclk().0 / 1_000_000);
        rprintln!(" HCLK: {:?} MHz", clocks.hclk().0 / 1_000_000);
        rprintln!(" APB1 clk: {:?} MHz", clocks.pclk1().0 / 1_000_000);
        rprintln!(" APB1 TIM: {:?} MHz", clocks.pclk1_tim().0 / 1_000_000);
        rprintln!(" APB2 clk: {:?} MHz", clocks.pclk2().0 / 1_000_000);
        rprintln!(" ADCCLK: {:?} MHz", clocks.adcclk().0 / 1_000_000);

        // Countdown timer setup.
        let mut scope_timer =
            timer::Timer::tim2(dp.TIM2, &clocks, &mut rcc.apb1).start_count_down(2.khz());
        scope_timer.listen(timer::Event::Update);

        // Peripheral setup.
        let mut afio = dp.AFIO.constrain(&mut rcc.apb2);
        let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
        let mut gpioc = dp.GPIOC.split(&mut rcc.apb2);

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
        blink_led::spawn().unwrap();
        read_input::spawn().unwrap();

        // Prevent wait-for-interrupt (default rtic idle) from stalling debug features.
        //
        // See: https://github.com/probe-rs/probe-rs/issues/350
        dp.DBGMCU.cr.modify(|_, w| {
            w.dbg_sleep().set_bit();
            w.dbg_standby().set_bit();
            w.dbg_stop().set_bit()
        });
        let _dma1 = dp.DMA1.split(&mut rcc.ahb);

        rprintln!("RTIC init completed");

        rprintln!("You may enter a PWM frequency in HZ for PA6:");

        init::LateResources {
            led,
            scope,
            scope_timer,
            led_pwm,
            rtt_down: rtt_channels.down.0,
        }
    }

    #[task(resources = [led])]
    fn blink_led(mut ctx: blink_led::Context) {
        ctx.resources.led.lock(|led| led.toggle().unwrap());

        // Schedule next blink.
        blink_led::schedule(ctx.scheduled + SYSCLK_HZ.cycles()).unwrap();
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
            rprintln!(
                "led_pwm duty = {}% ({} / {})",
                PWM_LEVELS[*pwm_level],
                duty,
                max_duty
            );
        });
    }

    #[task(resources = [rtt_down, led_pwm])]
    fn read_input(ctx: read_input::Context) {
        let read_input::Resources { rtt_down, led_pwm } = ctx.resources;

        (rtt_down, led_pwm).lock(|rtt_down, led_pwm| {
            let mut buf = [0u8; 100];
            let count = rtt_down.read(&mut buf);
            if count > 1 {
                // `count` bytes includes carriage return.
                let mut input_num = 0i32;
                for c in buf[..count - 1].iter() {
                    if '0' as u8 <= *c && *c <= '9' as u8 {
                        input_num *= 10;
                        input_num += (*c - '0' as u8) as i32;
                    } else {
                        rprintln!("invalid numeral: '{}'", *c as char);
                        input_num = -1;
                        break;
                    }
                }
                if input_num >= 0 {
                    rprintln!("Setting LED pwm frequency to: {}", input_num);
                    led_pwm.set_period((input_num as u32).hz());
                }
            }
        });

        read_input::schedule(ctx.scheduled + RTT_POLL_PERIOD.cycles()).unwrap();
    }
}

#[derive(Debug)]
pub enum Direction {
    Down,
    Up,
}

#[inline(never)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rprintln!("{}", info);
    loop {}
}
