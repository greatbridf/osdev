use core::{cell::UnsafeCell, marker::PhantomData};

/// A marker type that indicates that the type is not `Send`.
pub struct NotSend(PhantomData<*const ()>);

/// A marker type that indicates that the type is not `Sync`.
#[allow(dead_code)]
pub struct NotSync(PhantomData<UnsafeCell<()>>);

// SAFETY: This is a marker type that indicates that the type is not `Send`.
//         So no restrictions on `Sync` are needed.
unsafe impl Sync for NotSend {}
