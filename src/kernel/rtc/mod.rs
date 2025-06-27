use core::time::Duration;

use super::timer::{Instant, Ticks};
use alloc::sync::Arc;
use eonix_log::println_warn;
use eonix_sync::Spin;

static RTC: Spin<Option<Arc<dyn RealTimeClock>>> = Spin::new(None);

pub trait RealTimeClock: Send + Sync {
    fn now(&self) -> Instant;
}

impl Instant {
    pub fn now() -> Instant {
        RTC.lock().as_ref().map(|rtc| rtc.now()).unwrap_or_else(|| {
            let since_boot = Ticks::since_boot();
            let pseudo_now = Duration::from_secs((55 * 365 + 30) * 24 * 3600) + since_boot;

            Instant::new(pseudo_now.as_secs(), pseudo_now.subsec_nanos())
        })
    }
}

pub fn register_rtc(rtc: impl RealTimeClock + 'static) {
    let mut rtc_lock = RTC.lock();
    if rtc_lock.is_some() {
        println_warn!("RTC is already registered, ignoring new registration");
        return;
    }

    *rtc_lock = Some(Arc::new(rtc));
}
