use super::{strategy::LockStrategy, Spin, UCondVar};

pub struct SemaphoreStrategy<const MAX: usize = { core::usize::MAX }>;

#[allow(dead_code)]
impl<const MAX: usize> SemaphoreStrategy<MAX> {
    #[inline(always)]
    fn is_locked(data: &<Self as LockStrategy>::StrategyData) -> bool {
        let counter = data.counter.lock();
        *counter > 0
    }
}

pub struct SemaphoreData {
    counter: Spin<usize>,
    cv: UCondVar,
}

unsafe impl<const MAX: usize> LockStrategy for SemaphoreStrategy<MAX> {
    type StrategyData = SemaphoreData;
    type GuardContext = ();

    #[inline(always)]
    fn data() -> Self::StrategyData {
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

pub struct RwSemaphoreStrategy<const READ_MAX: isize = { core::isize::MAX }>;

#[allow(dead_code)]
impl<const READ_MAX: isize> RwSemaphoreStrategy<READ_MAX> {
    #[inline(always)]
    fn is_read_locked(data: &<Self as LockStrategy>::StrategyData) -> bool {
        let counter = data.counter.lock();
        *counter > 0
    }

    #[inline(always)]
    fn is_write_locked(data: &<Self as LockStrategy>::StrategyData) -> bool {
        let counter = data.counter.lock();
        *counter < 0
    }
}

pub struct RwSemaphoreData {
    counter: Spin<isize>,
    read_cv: UCondVar,
    write_cv: UCondVar,
}

unsafe impl<const READ_MAX: isize> LockStrategy for RwSemaphoreStrategy<READ_MAX> {
    type StrategyData = RwSemaphoreData;
    type GuardContext = ();

    #[inline(always)]
    unsafe fn is_locked(data: &Self::StrategyData) -> bool {
        *data.counter.lock() != 0
    }

    #[inline(always)]
    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext> {
        let mut counter = data.counter.lock();
        assert!(*counter >= -1 && *counter <= READ_MAX);
        if *counter == 0 {
            *counter -= 1;
            Some(())
        } else {
            None
        }
    }

    #[inline(always)]
    fn data() -> Self::StrategyData {
        RwSemaphoreData {
            counter: Spin::new(0),
            read_cv: UCondVar::new(),
            write_cv: UCondVar::new(),
        }
    }

    #[inline(always)]
    /// Acquire the semaphore in write mode
    ///
    /// # Might Sleep
    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        loop {
            let mut counter = data.counter.lock();
            assert!(*counter >= -1 && *counter <= READ_MAX);

            if *counter == 0 {
                *counter -= 1;
                return;
            }

            data.write_cv.wait(&mut counter);
        }
    }

    #[inline(always)]
    /// Acquire the semaphore in read mode
    ///
    /// # Might Sleep
    unsafe fn do_lock_shared(data: &Self::StrategyData) -> Self::GuardContext {
        loop {
            let mut counter = data.counter.lock();
            assert!(*counter >= -1 && *counter <= READ_MAX);

            if *counter >= 0 && *counter < READ_MAX {
                *counter += 1;
                return;
            }

            data.read_cv.wait(&mut counter);
        }
    }

    #[inline(always)]
    unsafe fn do_unlock(data: &Self::StrategyData, _: &mut Self::GuardContext) {
        let mut counter = data.counter.lock();
        assert!(*counter >= -1 && *counter <= READ_MAX);

        match *counter {
            -1 => {
                *counter = 0;
                data.read_cv.notify_all();
                data.write_cv.notify_one();
            }
            n if n > 0 => {
                *counter -= 1;
                if *counter == 0 {
                    data.write_cv.notify_one();
                }
            }
            _ => panic!("Semaphore in inconsistent state"),
        }
    }
}
