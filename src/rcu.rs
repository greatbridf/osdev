use crate::prelude::*;
use alloc::sync::Arc;
use core::{
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicPtr, Ordering},
};
use eonix_runtime::task::Task;
use eonix_sync::{Mutex, RwLock, RwLockReadGuard};
use pointers::BorrowedArc;

pub struct RCUReadGuard<'data, T: 'data> {
    value: T,
    _guard: RwLockReadGuard<'data, ()>,
    _phantom: PhantomData<&'data T>,
}

static GLOBAL_RCU_SEM: RwLock<()> = RwLock::new(());

impl<'data, T: 'data> RCUReadGuard<'data, T> {
    fn lock(value: T) -> Self {
        Self {
            value,
            _guard: Task::block_on(GLOBAL_RCU_SEM.read()),
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

pub trait RCUNode<MySelf> {
    fn rcu_prev(&self) -> &AtomicPtr<MySelf>;
    fn rcu_next(&self) -> &AtomicPtr<MySelf>;
}

pub struct RCUList<T: RCUNode<T>> {
    head: AtomicPtr<T>,

    reader_lock: RwLock<()>,
    update_lock: Mutex<()>,
}

impl<T: RCUNode<T>> RCUList<T> {
    pub const fn new() -> Self {
        Self {
            head: AtomicPtr::new(core::ptr::null_mut()),
            reader_lock: RwLock::new(()),
            update_lock: Mutex::new(()),
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

        let _lck = self.reader_lock.write();
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

        let _lck = self.reader_lock.write();
        old_node
            .rcu_prev()
            .store(core::ptr::null_mut(), Ordering::Release);
        old_node
            .rcu_next()
            .store(core::ptr::null_mut(), Ordering::Release);
    }

    pub fn iter(&self) -> RCUIterator<T> {
        let _lck = Task::block_on(self.reader_lock.read());

        RCUIterator {
            // SAFETY: We have a read lock, so the node is still alive.
            cur: NonNull::new(self.head.load(Ordering::SeqCst)),
            _lock: _lck,
        }
    }
}

pub struct RCUIterator<'lt, T: RCUNode<T>> {
    cur: Option<NonNull<T>>,
    _lock: RwLockReadGuard<'lt, ()>,
}

impl<'lt, T: RCUNode<T>> Iterator for RCUIterator<'lt, T> {
    type Item = BorrowedArc<'lt, T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cur {
            None => None,
            Some(pointer) => {
                // SAFETY: We have a read lock, so the node is still alive.
                let reference = unsafe { pointer.as_ref() };

                self.cur = NonNull::new(reference.rcu_next().load(Ordering::SeqCst));
                Some(unsafe { BorrowedArc::from_raw(pointer) })
            }
        }
    }
}

pub struct RCUPointer<T>(AtomicPtr<T>);

impl<T: core::fmt::Debug> core::fmt::Debug for RCUPointer<T> {
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

impl<T> RCUPointer<T> {
    pub fn empty() -> Self {
        Self(AtomicPtr::new(core::ptr::null_mut()))
    }

    pub fn load<'lt>(&self) -> Option<RCUReadGuard<'lt, BorrowedArc<'lt, T>>> {
        NonNull::new(self.0.load(Ordering::Acquire))
            .map(|p| RCUReadGuard::lock(unsafe { BorrowedArc::from_raw(p) }))
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
}

impl<T> Drop for RCUPointer<T> {
    fn drop(&mut self) {
        // SAFETY: We call `rcu_sync()` to ensure that all readers are done.
        if let Some(arc) = unsafe { self.swap(None) } {
            // We only wait if there are other references.
            if Arc::strong_count(&arc) == 1 {
                Task::block_on(rcu_sync());
            }
        }
    }
}
