use core::fmt;

pub trait PhysPtr {
    fn as_ptr<T>(&self) -> *mut T;

    fn as_ref<'lifetime, T>(&self) -> &'lifetime T {
        unsafe { &*(self.as_ptr()) }
    }

    fn as_mut<'lifetime, T>(&self) -> &'lifetime mut T {
        unsafe { &mut *(self.as_ptr()) }
    }

    fn as_slice<'lifetime, T>(&self, len: usize) -> &'lifetime [T] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), len) }
    }

    fn as_mut_slice<'lifetime, T>(&self, len: usize) -> &'lifetime mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.as_ptr(), len) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CachedPP {
    addr: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NoCachePP {
    addr: usize,
}

impl CachedPP {
    pub fn new(addr: usize) -> Self {
        Self { addr }
    }

    pub fn offset(&self, offset: usize) -> Self {
        Self {
            addr: self.addr + offset,
        }
    }
}

impl PhysPtr for CachedPP {
    fn as_ptr<T>(&self) -> *mut T {
        (self.addr + 0xffffff0000000000) as *mut T
    }
}

impl fmt::Debug for CachedPP {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CachedPP({:#x})", self.addr)
    }
}

impl NoCachePP {
    pub fn new(addr: usize) -> Self {
        Self { addr }
    }

    pub fn offset(&self, offset: isize) -> Self {
        Self {
            addr: self.addr + offset as usize,
        }
    }
}

impl PhysPtr for NoCachePP {
    fn as_ptr<T>(&self) -> *mut T {
        (self.addr + 0xffffff4000000000) as *mut T
    }
}

impl fmt::Debug for NoCachePP {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NoCachePP({:#x})", self.addr)
    }
}
