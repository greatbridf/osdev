use super::{Spin, UCondVar};
use eonix_sync::LockStrategy;

pub struct SemaphoreStrategy<const MAX: usize = { core::usize::MAX }>;

pub struct SemaphoreData {
    counter: Spin<usize>,
    cv: UCondVar,
}

unsafe impl<const MAX: usize> LockStrategy for SemaphoreStrategy<MAX> {
    type StrategyData = SemaphoreData;
    type GuardContext = ();

    #[inline(always)]
    fn new_data() -> Self::StrategyData {
        SemaphoreData {
            counter: Spin::new(0),
            cv: UCondVar::new(),
        }
    }

    #[inline(always)]
    unsafe fn is_locked(data: &Self::StrategyData) -> bool {
        *data.counter.lock() == MAX
    }

    #[inline(always)]
    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext> {
        let mut counter = data.counter.lock();
        assert!(*counter <= MAX);
        if *counter < MAX {
            *counter += 1;
            Some(())
        } else {
            None
        }
    }

    #[inline(always)]
    /// Acquire the semaphore in write mode
    ///
    /// # Might Sleep
    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        loop {
            let mut counter = data.counter.lock();
            assert!(*counter <= MAX);

            if *counter < MAX {
                *counter += 1;
                return;
            }

            data.cv.wait(&mut counter);
        }
    }

    #[inline(always)]
    unsafe fn do_unlock(data: &Self::StrategyData, _: &mut Self::GuardContext) {
        let mut counter = data.counter.lock();
        assert!(*counter <= MAX);

        match *counter {
            n if n > 0 => {
                *counter -= 1;
                data.cv.notify_one();
            }
            _ => panic!("Semaphore in inconsistent state"),
        }
    }
}
