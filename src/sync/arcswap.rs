use alloc::sync::Arc;
use core::{
    fmt::{self, Debug, Formatter},
    ptr::NonNull,
    sync::atomic::{AtomicPtr, Ordering},
};
use pointers::BorrowedArc;

unsafe impl<T> Send for ArcSwap<T> where T: Send + Sync {}
unsafe impl<T> Sync for ArcSwap<T> where T: Send + Sync {}

pub struct ArcSwap<T> {
    pointer: AtomicPtr<T>,
}

impl<T> ArcSwap<T> {
    pub fn new(data: T) -> Self {
        let pointer = Arc::into_raw(Arc::new(data));
        Self {
            pointer: AtomicPtr::new(pointer as *mut T),
        }
    }

    pub fn with_pointer(pointer: Arc<T>) -> Self {
        Self {
            pointer: AtomicPtr::new(Arc::into_raw(pointer) as *mut _),
        }
    }

    /// # Safety
    /// The caller must ensure that the pointer not used elsewhere before ACTUALLLY dropping that.
    pub fn swap(&self, data: Option<Arc<T>>) -> Option<Arc<T>> {
        let new_pointer = data.map(Arc::into_raw).unwrap_or(core::ptr::null());
        let old_pointer = self.pointer.swap(new_pointer as *mut _, Ordering::AcqRel);
        if old_pointer.is_null() {
            None
        } else {
            Some(unsafe { Arc::from_raw(old_pointer) })
        }
    }

    pub fn borrow(&self) -> BorrowedArc<T> {
        unsafe {
            BorrowedArc::from_raw(
                NonNull::new(self.pointer.load(Ordering::Acquire))
                    .expect("ArcSwap: pointer should not be null."),
            )
        }
    }

    pub fn try_borrow(&self) -> Option<BorrowedArc<T>> {
        NonNull::new(self.pointer.load(Ordering::Acquire))
            .map(|ptr| unsafe { BorrowedArc::from_raw(ptr) })
    }
}

impl<T> Debug for ArcSwap<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ArcSwap {{ {:?} }}", self.borrow().as_ref())
    }
}
