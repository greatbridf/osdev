use alloc::{collections::BinaryHeap, vec, vec::Vec};
use core::{
    cell::RefCell,
    cmp::Reverse,
    ops::Add,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Poll, Waker},
    time::Duration,
};
use eonix_hal::processor::CPU;
use eonix_sync::{Spin, SpinIrq as _};
use posix_types::stat::{StatXTimestamp, TimeSpec, TimeVal};

static TICKS: AtomicUsize = AtomicUsize::new(0);
static WAKEUP_TICK: AtomicUsize = AtomicUsize::new(usize::MAX);
static SLEEPERS_LIST: Spin<BinaryHeap<Reverse<Sleepers>>> = Spin::new(BinaryHeap::new());

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct Ticks(usize);

#[derive(Debug, Default, Clone, Copy)]
pub struct Instant {
    secs_since_epoch: u64,
    nsecs_within: u32,
}

struct Sleepers {
    wakeup_tick: Ticks,
    wakers: RefCell<Vec<Waker>>,
}

impl Ord for Sleepers {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.wakeup_tick.cmp(&other.wakeup_tick)
    }
}

impl PartialOrd for Sleepers {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for Sleepers {}

impl PartialEq for Sleepers {
    fn eq(&self, other: &Self) -> bool {
        self.wakeup_tick == other.wakeup_tick
    }
}

impl Ticks {
    pub const fn in_secs(&self) -> u64 {
        self.0 as u64 / 1_000
    }

    pub const fn in_msecs(&self) -> u128 {
        self.0 as u128
    }

    pub const fn in_usecs(&self) -> u128 {
        self.0 as u128 * 1_000
    }

    pub const fn in_nsecs(&self) -> u128 {
        self.0 as u128 * 1_000_000
    }

    pub fn now() -> Self {
        Ticks(TICKS.load(Ordering::Acquire))
    }

    pub fn since_boot() -> Duration {
        Duration::from_nanos(Self::now().in_nsecs() as u64)
    }
}

impl Instant {
    pub const fn default() -> Self {
        Instant {
            secs_since_epoch: 0,
            nsecs_within: 0,
        }
    }

    pub fn new(secs_since_epoch: u64, nsecs_within: u32) -> Self {
        Instant {
            secs_since_epoch,
            nsecs_within,
        }
    }

    pub fn elapsed(&self) -> Duration {
        let now = Instant::now();
        if now.nsecs_within < self.nsecs_within {
            // We have wrapped around the nanoseconds.
            Duration::new(
                now.secs_since_epoch - self.secs_since_epoch - 1,
                1_000_000_000 + now.nsecs_within - self.nsecs_within,
            )
        } else {
            Duration::new(
                now.secs_since_epoch - self.secs_since_epoch,
                now.nsecs_within - self.nsecs_within,
            )
        }
    }

    pub fn since_epoch(&self) -> Duration {
        Duration::new(self.secs_since_epoch, self.nsecs_within)
    }
}

impl From<Instant> for TimeSpec {
    fn from(value: Instant) -> Self {
        Self {
            tv_sec: value.secs_since_epoch,
            tv_nsec: value.nsecs_within,
        }
    }
}

impl From<Instant> for TimeVal {
    fn from(value: Instant) -> Self {
        Self {
            tv_sec: value.secs_since_epoch,
            tv_usec: value.nsecs_within / 1_000,
        }
    }
}

impl From<Instant> for StatXTimestamp {
    fn from(value: Instant) -> Self {
        Self {
            tv_sec: value.secs_since_epoch,
            tv_nsec: value.nsecs_within,
        }
    }
}

impl Add for Ticks {
    type Output = Ticks;

    fn add(self, other: Self) -> Self::Output {
        Ticks(self.0 + other.0)
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, duration: Duration) -> Self::Output {
        let nsecs = self.nsecs_within + duration.subsec_nanos();
        let nsecs_within = nsecs % 1_000_000_000;
        let secs_since_epoch =
            self.secs_since_epoch + duration.as_secs() + (nsecs / 1_000_000_000) as u64;

        Instant {
            secs_since_epoch,
            nsecs_within: nsecs_within,
        }
    }
}

pub fn timer_interrupt() {
    if CPU::local().cpuid() != 0 {
        // Only the BSP should handle the timer interrupt.
        return;
    }

    let current_tick = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    let wakeup_tick = WAKEUP_TICK.load(Ordering::Acquire);

    if wakeup_tick <= current_tick {
        let mut sleepers = SLEEPERS_LIST.lock_irq();
        let Some(Reverse(sleepers_to_wakeup)) = sleepers.pop() else {
            return;
        };

        for waker in sleepers_to_wakeup.wakers.into_inner() {
            waker.wake();
        }

        if WAKEUP_TICK.load(Ordering::Acquire) == wakeup_tick {
            // The wakeup tick is not changed.
            // Set the next wakeup tick to the next sleeper's wakeup time.
            let wakeup_tick = sleepers
                .peek()
                .map(|sleepers| sleepers.0.wakeup_tick.0)
                .unwrap_or(usize::MAX);

            WAKEUP_TICK.store(wakeup_tick, Ordering::Release);
        }
    }
}

/// Returns true if the timeslice of the current task has expired and it should be rescheduled.
pub fn should_reschedule() -> bool {
    #[eonix_percpu::define_percpu]
    static PREV_SCHED_TICK: usize = 0;

    let prev_tick = PREV_SCHED_TICK.get();
    let current_tick = Ticks::now().0;

    if Ticks(current_tick - prev_tick).in_msecs() >= 10 {
        PREV_SCHED_TICK.set(current_tick);
        true
    } else {
        false
    }
}

pub async fn sleep(duration: Duration) {
    let wakeup_tick = Ticks::now() + Ticks(duration.as_millis() as usize);

    core::future::poll_fn(|ctx| {
        if Ticks::now() >= wakeup_tick {
            return Poll::Ready(());
        }

        let mut sleepers_list = SLEEPERS_LIST.lock_irq();
        let sleepers: Option<&Reverse<Sleepers>> = sleepers_list
            .iter()
            .find(|s| s.0.wakeup_tick == wakeup_tick);

        match sleepers {
            Some(Reverse(sleepers)) => {
                sleepers.wakers.borrow_mut().push(ctx.waker().clone());
            }
            None => {
                sleepers_list.push(Reverse(Sleepers {
                    wakeup_tick,
                    wakers: RefCell::new(vec![ctx.waker().clone()]),
                }));
            }
        }

        if wakeup_tick < Ticks(WAKEUP_TICK.load(Ordering::Acquire)) {
            WAKEUP_TICK.store(wakeup_tick.0, Ordering::Release);
        }

        Poll::Pending
    })
    .await;
}
