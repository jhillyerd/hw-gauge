use core::convert::TryInto;

// RTIC Monotonic impl for the 16-bit timers
use defmt::println;
pub use fugit::{self, ExtU32};
use rtic_monotonic::Monotonic;
use stm32f1xx_hal::{
    pac::{RCC, TIM2},
    rcc::Clocks,
};

pub struct MonoTimer<T, const FREQ: u32>(T);

impl<const FREQ: u32> MonoTimer<TIM2, FREQ> {
    pub fn new(timer: TIM2, clocks: &Clocks) -> Self {
        // Enable timer.
        let rcc = unsafe { &(*RCC::ptr()) };
        rcc.apb1enr.modify(|_, w| w.tim2en().set_bit());
        rcc.apb1rstr.modify(|_, w| w.tim2rst().set_bit());
        rcc.apb1rstr.modify(|_, w| w.tim2rst().clear_bit());

        // Configure timer.
        let ticks = clocks.pclk1_tim().0 / FREQ;
        let psc: u16 = ((ticks - 1) / (1 << 16)).try_into().unwrap();
        let arr: u16 = (ticks / (psc + 1) as u32).try_into().unwrap();
        println!("ticks {}, psc {}, arr {}", ticks, psc, arr); // ticks 720000, psc 10, arr 65454
        timer.psc.write(|w| w.psc().bits(psc));
        timer.arr.write(|w| w.arr().bits(arr));

        timer.egr.write(|w| w.ug().set_bit()); // Reset timer.
        timer.sr.modify(|_, w| w.uif().clear_bit()); // Clear interrupt flag.
        timer.cr1.modify(|_, w| w.cen().set_bit()); // Start timer.

        MonoTimer(timer)
    }
}

impl<const FREQ: u32> Monotonic for MonoTimer<TIM2, FREQ> {
    type Instant = fugit::TimerInstantU32<FREQ>;
    type Duration = fugit::TimerDurationU32<FREQ>;

    unsafe fn reset(&mut self) {
        self.0.dier.modify(|_, w| w.cc1ie().set_bit());
    }

    #[inline(always)]
    fn now(&mut self) -> Self::Instant {
        Self::Instant::from_ticks(self.0.cnt.read().cnt().bits().into())
    }

    fn set_compare(&mut self, instant: Self::Instant) {
        self.0.ccr1.write(|w| {
            w.ccr()
                .bits(instant.duration_since_epoch().ticks().try_into().unwrap())
        });
    }

    fn clear_compare_flag(&mut self) {
        self.0.sr.modify(|_, w| w.cc1if().clear_bit());
    }

    #[inline(always)]
    fn zero() -> Self::Instant {
        Self::Instant::from_ticks(0)
    }
}
