pub struct Locked<T: Sized + Sync, U: ?Sized> {
    inner: T,
    guard: *const U,
}

unsafe impl<T: Sized + Sync, U: ?Sized> Sync for Locked<T, U> {}
unsafe impl<T: Sized + Sync, U: ?Sized> Send for Locked<T, U> {}

impl<T: Sized + Sync, U: ?Sized> Locked<T, U> {
    pub fn new(value: T, from: &U) -> Self {
        Self {
            inner: value,
            guard: from,
        }
    }

    pub fn access<'lt>(&'lt self, guard: &'lt U) -> &'lt T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        &self.inner
    }

    pub fn access_mut<'lt>(&'lt self, guard: &'lt mut U) -> &'lt mut T {
        assert_eq!(self.guard, guard as *const U, "wrong guard");
        unsafe { &mut *(&raw const self.inner as *mut T) }
    }
}
