use crate::{kernel::task::block_on, prelude::*};
use alloc::sync::Arc;
use arcref::ArcRef;
use core::{
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicPtr, Ordering},
};
use eonix_preempt::PreemptGuard;
use eonix_runtime::scheduler::RUNTIME;
use eonix_sync::{RwLock, RwLockReadGuard};
use pointers::BorrowedArc;

/// The RCU Read Lock. Holding a reference to an instance of the struct assures
/// you that any RCU protected data would not be dropped.
///
/// The struct cannot be created directly. Instead, use [`rcu_read_lock()`].
#[derive(Debug)]
pub struct RCUReadLock();

pub struct RCUReadGuardNew {
    guard: RwLockReadGuard<'static, RCUReadLock>,
    _disable_preempt: PreemptGuard<()>,
}

pub struct RCUReadGuard<'data, T: 'data> {
    value: T,
    _guard: RwLockReadGuard<'static, RCUReadLock>,
    _phantom: PhantomData<&'data T>,
}

static GLOBAL_RCU_SEM: RwLock<RCUReadLock> = RwLock::new(RCUReadLock());

impl<'data, T> RCUReadGuard<'data, BorrowedArc<'data, T>> {
    fn lock(value: BorrowedArc<'data, T>) -> Self {
        Self {
            value,
            _guard: block_on(GLOBAL_RCU_SEM.read()),
            _phantom: PhantomData,
        }
    }
}

impl<'data, T: 'data> Deref for RCUReadGuard<'data, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

pub async fn rcu_sync() {
    // Lock the global RCU semaphore to ensure that all readers are done.
    let _ = GLOBAL_RCU_SEM.write().await;
}

pub fn call_rcu(func: impl FnOnce() + Send + 'static) {
    RUNTIME.spawn(async move {
        // Wait for all readers to finish.
        rcu_sync().await;
        func();
    });
}

pub trait RCUNode<MySelf> {
    fn rcu_prev(&self) -> &AtomicPtr<MySelf>;
    fn rcu_next(&self) -> &AtomicPtr<MySelf>;
}

pub struct RCUList<T: RCUNode<T>> {
    head: AtomicPtr<T>,
    update_lock: Spin<()>,
}

impl<T: RCUNode<T>> RCUList<T> {
    pub const fn new() -> Self {
        Self {
            head: AtomicPtr::new(core::ptr::null_mut()),
            update_lock: Spin::new(()),
        }
    }

    pub fn insert(&self, new_node: Arc<T>) {
        let _lck = self.update_lock.lock();

        let old_head = self.head.load(Ordering::Acquire);
        new_node
            .rcu_prev()
            .store(core::ptr::null_mut(), Ordering::Release);
        new_node.rcu_next().store(old_head, Ordering::Release);

        if let Some(old_head) = unsafe { old_head.as_ref() } {
            old_head
                .rcu_prev()
                .store(Arc::into_raw(new_node.clone()) as *mut _, Ordering::Release);
        }

        self.head
            .store(Arc::into_raw(new_node) as *mut _, Ordering::Release);
    }

    pub fn remove(&self, node: &Arc<T>) {
        let _lck = self.update_lock.lock();

        let prev = node.rcu_prev().load(Ordering::Acquire);
        let next = node.rcu_next().load(Ordering::Acquire);

        if let Some(next) = unsafe { next.as_ref() } {
            let me = next.rcu_prev().swap(prev, Ordering::AcqRel);
            debug_assert!(me == Arc::as_ptr(&node) as *mut _);
            unsafe { Arc::from_raw(me) };
        }

        {
            let prev_next =
                unsafe { prev.as_ref().map(|rcu| rcu.rcu_next()) }.unwrap_or(&self.head);

            let me = prev_next.swap(next, Ordering::AcqRel);
            debug_assert!(me == Arc::as_ptr(&node) as *mut _);
            unsafe { Arc::from_raw(me) };
        }

        node.rcu_prev()
            .store(core::ptr::null_mut(), Ordering::Release);
        node.rcu_next()
            .store(core::ptr::null_mut(), Ordering::Release);
    }

    pub fn replace(&self, old_node: &Arc<T>, new_node: Arc<T>) {
        let _lck = self.update_lock.lock();

        let prev = old_node.rcu_prev().load(Ordering::Acquire);
        let next = old_node.rcu_next().load(Ordering::Acquire);

        new_node.rcu_prev().store(prev, Ordering::Release);
        new_node.rcu_next().store(next, Ordering::Release);

        {
            let prev_next =
                unsafe { prev.as_ref().map(|rcu| rcu.rcu_next()) }.unwrap_or(&self.head);

            let old = prev_next.swap(Arc::into_raw(new_node.clone()) as *mut _, Ordering::AcqRel);

            debug_assert!(old == Arc::as_ptr(&old_node) as *mut _);
            unsafe { Arc::from_raw(old) };
        }

        if let Some(next) = unsafe { next.as_ref() } {
            let old = next
                .rcu_prev()
                .swap(Arc::into_raw(new_node.clone()) as *mut _, Ordering::AcqRel);

            debug_assert!(old == Arc::as_ptr(&old_node) as *mut _);
            unsafe { Arc::from_raw(old) };
        }

        old_node
            .rcu_prev()
            .store(core::ptr::null_mut(), Ordering::Release);
        old_node
            .rcu_next()
            .store(core::ptr::null_mut(), Ordering::Release);
    }

    pub fn iter<'a, 'r>(&'a self, _lock: &'r RCUReadLock) -> RCUIterator<'a, 'r, T> {
        RCUIterator {
            cur: NonNull::new(self.head.load(Ordering::Acquire)),
            _phantom: PhantomData,
        }
    }
}

pub struct RCUIterator<'list, 'rcu, T: RCUNode<T>> {
    cur: Option<NonNull<T>>,
    _phantom: PhantomData<(&'list (), &'rcu ())>,
}

impl<'rcu, T: RCUNode<T>> Iterator for RCUIterator<'_, 'rcu, T> {
    type Item = ArcRef<'rcu, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur.map(|pointer| {
            let reference = unsafe {
                // SAFETY: We have the read lock so the node is still alive.
                pointer.as_ref()
            };

            self.cur = NonNull::new(reference.rcu_next().load(Ordering::Acquire));

            unsafe {
                // SAFETY: We have the read lock so the node is still alive.
                ArcRef::new_unchecked(pointer.as_ptr())
            }
        })
    }
}

pub struct RCUPointer<T>(AtomicPtr<T>)
where
    T: Send + Sync + 'static;

impl<T> core::fmt::Debug for RCUPointer<T>
where
    T: core::fmt::Debug,
    T: Send + Sync + 'static,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match NonNull::new(self.0.load(Ordering::Acquire)) {
            Some(pointer) => {
                let borrowed = unsafe { BorrowedArc::from_raw(pointer) };
                f.write_str("RCUPointer of ")?;
                borrowed.fmt(f)
            }
            None => f.debug_tuple("NULL RCUPointer").finish(),
        }
    }
}

impl<T> RCUPointer<T>
where
    T: Send + Sync + 'static,
{
    pub const fn empty() -> Self {
        Self(AtomicPtr::new(core::ptr::null_mut()))
    }

    pub fn new(value: Arc<T>) -> Self {
        Self(AtomicPtr::new(Arc::into_raw(value) as *mut T))
    }

    pub fn load<'lt>(&self) -> Option<RCUReadGuard<'lt, BorrowedArc<'lt, T>>> {
        // BUG: We should acquire the lock before loading the pointer
        NonNull::new(self.0.load(Ordering::Acquire))
            .map(|p| RCUReadGuard::lock(unsafe { BorrowedArc::from_raw(p) }))
    }

    pub fn dereference<'r, 'a: 'r>(&self, _lock: &'a RCUReadLock) -> Option<ArcRef<'r, T>> {
        NonNull::new(self.0.load(Ordering::Acquire)).map(|p| unsafe {
            // SAFETY: We have a read lock, so the node is still alive.
            ArcRef::new_unchecked(p.as_ptr())
        })
    }

    /// # Safety
    /// Caller must ensure no writers are updating the pointer.
    pub unsafe fn load_locked<'lt>(&self) -> Option<BorrowedArc<'lt, T>> {
        NonNull::new(self.0.load(Ordering::Acquire)).map(|p| unsafe { BorrowedArc::from_raw(p) })
    }

    /// # Safety
    /// Caller must ensure that the actual pointer is freed after all readers are done.
    pub unsafe fn swap(&self, new: Option<Arc<T>>) -> Option<Arc<T>> {
        let new = new
            .map(|arc| Arc::into_raw(arc) as *mut T)
            .unwrap_or(core::ptr::null_mut());

        let old = self.0.swap(new, Ordering::AcqRel);

        if old.is_null() {
            None
        } else {
            Some(unsafe { Arc::from_raw(old) })
        }
    }

    /// Exchange the value of the pointers.
    ///
    /// # Safety
    /// Presence of readers is acceptable.
    /// But the caller must ensure that we are the only one **altering** the pointers.
    pub unsafe fn exchange(old: &Self, new: &Self) {
        let old_value = old.0.load(Ordering::Acquire);

        let new_value = new.0.swap(old_value, Ordering::AcqRel);

        old.0.store(new_value, Ordering::Release);
    }
}

impl<T> Drop for RCUPointer<T>
where
    T: Send + Sync + 'static,
{
    fn drop(&mut self) {
        // SAFETY: We call `rcu_sync()` to ensure that all readers are done.
        if let Some(arc) = unsafe { self.swap(None) } {
            // We only wait if there are other references.
            if Arc::strong_count(&arc) == 1 {
                call_rcu(move || drop(arc));
            }
        }
    }
}

impl Deref for RCUReadGuardNew {
    type Target = RCUReadLock;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

pub fn rcu_read_lock() -> RCUReadGuardNew {
    RCUReadGuardNew {
        guard: block_on(GLOBAL_RCU_SEM.read()),
        _disable_preempt: PreemptGuard::new(()),
    }
}
