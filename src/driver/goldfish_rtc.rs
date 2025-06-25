use crate::kernel::{
    rtc::{register_rtc, RealTimeClock},
    timer::Instant,
};
use core::ptr::NonNull;
use eonix_hal::{arch_exported::fdt::FDT, mm::ArchPhysAccess};
use eonix_log::println_warn;
use eonix_mm::address::{PAddr, PhysAccess};

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
    let Some(rtc) = FDT.find_compatible(&["google,goldfish-rtc"]) else {
        println_warn!("Goldfish RTC not found in FDT");
        return;
    };

    let mut regs = rtc.reg().expect("Goldfish RTC reg not found");
    let base = regs
        .next()
        .map(|r| PAddr::from(r.starting_address as usize))
        .expect("Goldfish RTC base address not found");

    let goldfish_rtc = GoldfishRtc {
        time_low: unsafe { ArchPhysAccess::as_ptr(base) },
        time_high: unsafe { ArchPhysAccess::as_ptr(base + 4) },
    };

    register_rtc(goldfish_rtc);
}
