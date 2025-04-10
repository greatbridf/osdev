use crate::{Relax, SpinRelax};
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicU8, Ordering},
};

enum LazyState<T, F>
where
    F: FnOnce() -> T,
{
    Uninitialized(F),
    Initializing,
    Initialized(T),
}

pub struct LazyLock<T, F = fn() -> T, R = SpinRelax>
where
    F: FnOnce() -> T,
    R: Relax,
{
    value: UnsafeCell<LazyState<T, F>>,
    state: AtomicU8,
    _phantom: PhantomData<R>,
}

unsafe impl<T, F, R> Sync for LazyLock<T, F, R>
where
    T: Send + Sync,
    F: FnOnce() -> T,
    F: Send,
    R: Relax,
{
}

impl<T, F, R> LazyLock<T, F, R>
where
    F: FnOnce() -> T,
    R: Relax,
{
    const UNINITIALIZED: u8 = 0;
    const INITIALIZING: u8 = 1;
    const INITIALIZED: u8 = 2;

    pub const fn new(init: F) -> Self {
        Self {
            value: UnsafeCell::new(LazyState::Uninitialized(init)),
            state: AtomicU8::new(Self::UNINITIALIZED),
            _phantom: PhantomData,
        }
    }

    /// # Safety
    /// We should sync with the writer when calling this function or we could read stale data.
    unsafe fn get_initialized_value(&self) -> Option<&T> {
        // SAFETY: We're synced with the cpu that initialized it.
        if let LazyState::Initialized(value) = unsafe { &*self.value.get() } {
            Some(value)
        } else {
            None
        }
    }

    /// Does the initialization of the value and leave `self.state` untouched. The caller
    /// should set the state to `INITIALIZED` after calling this function.
    ///
    /// # Safety
    /// This function is unsafe because concurrent calls would result in undefined behavior.
    /// We should call this function exactly once with `self.state == INITIALIZING`.
    unsafe fn do_initialization(&self) {
        // SAFETY: We are the only thread that can access the value initializer.
        let stateref = unsafe { &mut *self.value.get() };
        let mut state = LazyState::Initializing;
        core::mem::swap(stateref, &mut state);

        if let LazyState::Uninitialized(init_func) = state {
            state = LazyState::Initialized(init_func());
        } else {
            unreachable!("Invalid LazyLock state.");
        };

        core::mem::swap(stateref, &mut state);
    }

    /// Spin until the value is initialized. Guarantees that the initialized value is
    /// visible to the caller cpu.
    fn spin_until_initialized(&self) {
        while self.state.load(Ordering::Acquire) != Self::INITIALIZED {
            R::relax();
        }
    }

    /// Get immutable reference to the wrapped value if initialized. Block until
    /// the value is initialized by someone (including the caller itself) otherwise.
    pub fn get(&self) -> &T {
        match self.state.load(Ordering::Acquire) {
            Self::UNINITIALIZED => match self.state.compare_exchange(
                Self::UNINITIALIZED,
                Self::INITIALIZING,
                Ordering::Acquire,
                Ordering::Acquire,
            ) {
                Ok(_) => unsafe {
                    // SAFETY: We are the only thread doing initialization.
                    self.do_initialization();
                    self.state.store(Self::INITIALIZED, Ordering::Release);
                },
                Err(Self::INITIALIZING) => self.spin_until_initialized(),
                Err(Self::INITIALIZED) => {}
                Err(_) => unreachable!("Invalid LazyLock state."),
            },
            Self::INITIALIZING => self.spin_until_initialized(),
            Self::INITIALIZED => {}
            _ => unreachable!("Invalid LazyLock state."),
        }

        unsafe {
            // SAFETY: If we're the spin waiter, we're synced with the cpu that initialized
            //         it using `Acquire`. If we're the one that initialized it, no
            //         synchronization is needed.
            self.get_initialized_value()
                .expect("Value should be initialized.")
        }
    }
}

impl<T, F, R> Deref for LazyLock<T, F, R>
where
    F: FnOnce() -> T,
    R: Relax,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T, U, F, R> AsRef<U> for LazyLock<T, F, R>
where
    U: ?Sized,
    F: FnOnce() -> T,
    R: Relax,
    <Self as Deref>::Target: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.deref().as_ref()
    }
}
