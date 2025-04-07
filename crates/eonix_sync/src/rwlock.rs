use crate::{LockStrategy, WaitStrategy};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicIsize, Ordering},
};

pub struct RwLockStrategy<W>(PhantomData<W>)
where
    W: WaitStrategy;

pub struct RwLockData<W>
where
    W: WaitStrategy,
{
    counter: AtomicIsize,
    wait_data: W::Data,
}

impl<W> RwLockStrategy<W>
where
    W: WaitStrategy,
{
    #[cold]
    fn lock_slow_path(
        data: &<Self as LockStrategy>::StrategyData,
    ) -> <Self as LockStrategy>::GuardContext {
        loop {
            if let Ok(_) =
                data.counter
                    .compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed)
            {
                return ();
            }

            W::write_wait(&data.wait_data, || {
                data.counter.load(Ordering::Relaxed) == 0
            });
        }
    }

    #[cold]
    fn lock_shared_slow_path(
        data: &<Self as LockStrategy>::StrategyData,
    ) -> <Self as LockStrategy>::GuardContext {
        loop {
            let mut counter = data.counter.load(Ordering::Relaxed);
            while counter >= 0 {
                match data.counter.compare_exchange_weak(
                    counter,
                    counter + 1,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return (),
                    Err(previous) => counter = previous,
                }
            }

            W::read_wait(&data.wait_data, || {
                data.counter.load(Ordering::Relaxed) >= 0
            });
        }
    }
}

unsafe impl<W> LockStrategy for RwLockStrategy<W>
where
    W: WaitStrategy,
{
    type StrategyData = RwLockData<W>;
    type GuardContext = ();

    fn new_data() -> Self::StrategyData {
        Self::StrategyData {
            counter: AtomicIsize::new(0),
            wait_data: W::new_data(),
        }
    }

    unsafe fn is_locked(data: &Self::StrategyData) -> bool {
        data.counter.load(Ordering::Relaxed) == 1
    }

    unsafe fn try_lock(data: &Self::StrategyData) -> Option<Self::GuardContext> {
        data.counter
            .compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| ())
            .ok()
    }

    unsafe fn try_lock_shared(data: &Self::StrategyData) -> Option<Self::GuardContext>
    where
        Self: Sized,
    {
        if W::has_write_waiting(&data.wait_data) {
            return None;
        }

        let counter = data.counter.load(Ordering::Relaxed);
        match counter {
            0.. => data
                .counter
                .compare_exchange(counter, counter + 1, Ordering::Acquire, Ordering::Relaxed)
                .ok()
                .map(|_| ()),
            _ => None,
        }
    }

    unsafe fn do_lock(data: &Self::StrategyData) -> Self::GuardContext {
        if let Some(context) = unsafe { Self::try_lock(data) } {
            // Quick path
            context
        } else {
            Self::lock_slow_path(data)
        }
    }

    unsafe fn do_lock_shared(data: &Self::StrategyData) -> Self::GuardContext {
        if let Some(context) = unsafe { Self::try_lock_shared(data) } {
            // Quick path
            context
        } else {
            Self::lock_shared_slow_path(data)
        }
    }

    unsafe fn do_unlock(data: &Self::StrategyData, _: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        let old = data.counter.fetch_add(1, Ordering::Release);
        assert_eq!(
            old, -1,
            "RwLockStrategy::do_unlock: erroneous counter value: {}",
            old
        );
        W::write_notify(&data.wait_data);
    }

    unsafe fn do_unlock_shared(data: &Self::StrategyData, _: &mut Self::GuardContext)
    where
        Self: Sized,
    {
        match data.counter.fetch_sub(1, Ordering::Release) {
            2.. => {}
            1 => W::read_notify(&data.wait_data),
            val => unreachable!(
                "RwLockStrategy::do_unlock_shared: erroneous counter value: {}",
                val
            ),
        }
    }
}
