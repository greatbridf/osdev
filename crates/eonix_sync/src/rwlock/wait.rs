pub trait Wait {
    fn new() -> Self
    where
        Self: Sized;

    fn has_write_waiting(&self) -> bool
    where
        Self: Sized;
    fn has_read_waiting(&self) -> bool
    where
        Self: Sized;

    fn write_wait(&self, check: impl Fn() -> bool)
    where
        Self: Sized;
    fn read_wait(&self, check: impl Fn() -> bool)
    where
        Self: Sized;

    fn write_notify(&self)
    where
        Self: Sized;
    fn read_notify(&self)
    where
        Self: Sized;
}
