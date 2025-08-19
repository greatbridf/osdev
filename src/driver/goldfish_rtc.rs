use crate::kernel::{
    rtc::{register_rtc, RealTimeClock},
    timer::Instant,
};
use core::ptr::NonNull;
use eonix_hal::{mm::ArchPhysAccess, platform::RTC_BASE};
use eonix_mm::address::PhysAccess;

#[cfg(not(target_arch = "riscv64"))]
compile_error!("Goldfish RTC driver is only supported on RISC-V architecture");

struct GoldfishRtc {
    time_low: NonNull<u32>,
    time_high: NonNull<u32>,
}

unsafe impl Send for GoldfishRtc {}
unsafe impl Sync for GoldfishRtc {}

impl RealTimeClock for GoldfishRtc {
    fn now(&self) -> Instant {
        // SAFETY: The pointer is guaranteed to be valid as long as the RTC is registered.
        let time_high = unsafe { self.time_high.read_volatile() };
        let time_low = unsafe { self.time_low.read_volatile() };

        let nsecs = ((time_high as u64) << 32) | (time_low as u64);
        let secs_since_epoch = nsecs / 1_000_000_000;
        let nsecs_within = nsecs % 1_000_000_000;

        Instant::new(secs_since_epoch as u64, nsecs_within as u32)
    }
}

pub fn probe() {
    let goldfish_rtc = GoldfishRtc {
        time_low: unsafe { ArchPhysAccess::as_ptr(RTC_BASE) },
        time_high: unsafe { ArchPhysAccess::as_ptr(RTC_BASE + 4) },
    };

    register_rtc(goldfish_rtc);
}
