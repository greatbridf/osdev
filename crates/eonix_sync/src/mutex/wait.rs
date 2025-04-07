pub trait Wait {
    fn new() -> Self
    where
        Self: Sized;

    fn has_waiting(&self) -> bool
    where
        Self: Sized;

    fn wait(&self, check: impl Fn() -> bool)
    where
        Self: Sized;

    fn notify(&self)
    where
        Self: Sized;
}
