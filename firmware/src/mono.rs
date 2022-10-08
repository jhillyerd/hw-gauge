use core::convert::TryInto;

// RTIC Monotonic impl for 16-bit timers.
pub use fugit::{self, ExtU32};
use rtic_monotonic::Monotonic;
use stm32f1xx_hal::{
    pac::{RCC, TIM2},
    rcc::Clocks,
};

pub struct MonoTimer<T, const FREQ: u32> {
    tim: T,
    ovf: u32,
}

impl<const FREQ: u32> MonoTimer<TIM2, FREQ> {
    pub fn new(timer: TIM2, clocks: &Clocks) -> Self {
        // Enable timer.
        let rcc = unsafe { &(*RCC::ptr()) };
        rcc.apb1enr.modify(|_, w| w.tim2en().set_bit());
        rcc.apb1rstr.modify(|_, w| w.tim2rst().set_bit());
        rcc.apb1rstr.modify(|_, w| w.tim2rst().clear_bit());

        // Configure timer.  If the u16 conversion panics, try increasing FREQ.
        let psc: u16 = (clocks.pclk1_tim().to_Hz() / FREQ - 1).try_into().unwrap();
        timer.psc.write(|w| w.psc().bits(psc)); // Set prescaler.
        timer.arr.write(|w| w.arr().bits(u16::MAX)); // Set auto-reload value.
        timer.egr.write(|w| w.ug().set_bit()); // Generate interrupt on overflow.

        // Start timer.
        timer.sr.modify(|_, w| w.uif().clear_bit()); // Clear interrupt flag.
        timer.cr1.modify(|_, w| {
            w.cen()
                .set_bit() // Enable counter.
                .udis()
                .clear_bit() // Overflow should trigger update event.
                .urs()
                .set_bit() // Only overflow triggers interrupt.
        });

        MonoTimer { tim: timer, ovf: 0 }
    }
}

impl<const FREQ: u32> Monotonic for MonoTimer<TIM2, FREQ> {
    type Instant = fugit::TimerInstantU32<FREQ>;
    type Duration = fugit::TimerDurationU32<FREQ>;

    unsafe fn reset(&mut self) {
        self.tim.dier.modify(|_, w| w.cc1ie().set_bit());
    }

    #[inline(always)]
    fn now(&mut self) -> Self::Instant {
        let cnt = self.tim.cnt.read().cnt().bits() as u32;

        // If the overflow bit is set, we add this to the timer value. It means the `on_interrupt`
        // has not yet happened, and we need to compensate here.
        let ovf = if self.tim.sr.read().uif().bit_is_set() {
            0x10000
        } else {
            0
        };

        Self::Instant::from_ticks(cnt.wrapping_add(ovf).wrapping_add(self.ovf))
    }

    fn set_compare(&mut self, instant: Self::Instant) {
        let now = self.now();
        let cnt = self.tim.cnt.read().cnt().bits();

        // Since the timer may or may not overflow based on the requested compare val, we check
        // how many ticks are left.
        let val = match instant.checked_duration_since(now) {
            None => cnt.wrapping_add(0xffff), // In the past, RTIC will handle this
            Some(x) if x.ticks() <= 0xffff => instant.duration_since_epoch().ticks() as u16, // Will not overflow
            Some(_) => cnt.wrapping_add(0xffff), // Will overflow, run for as long as possible
        };

        self.tim.ccr1.write(|w| w.ccr().bits(val));
    }

    fn clear_compare_flag(&mut self) {
        self.tim.sr.modify(|_, w| w.cc1if().clear_bit());
    }

    fn on_interrupt(&mut self) {
        // If there was an overflow, increment the overflow counter.
        if self.tim.sr.read().uif().bit_is_set() {
            self.tim.sr.modify(|_, w| w.uif().clear_bit());

            self.ovf += 0x10000;
        }
    }

    #[inline(always)]
    fn zero() -> Self::Instant {
        Self::Instant::from_ticks(0)
    }
}
