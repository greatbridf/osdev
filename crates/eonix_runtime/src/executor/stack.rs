use core::ptr::NonNull;

pub trait Stack: Sized + Send {
    fn new() -> Option<Self>;
    fn get_bottom(&self) -> NonNull<()>;
}
