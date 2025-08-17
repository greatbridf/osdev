// Fork form asterinas
pub const FD_SETSIZE: usize = 1024;
pub const USIZE_BITS: usize = core::mem::size_of::<usize>() * 8;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct FDSet {
    fds_bits: [usize; FD_SETSIZE / USIZE_BITS],
}

impl FDSet {
    /// Equivalent to FD_SET.
    pub fn set(&mut self, fd: u32) -> bool {
        let fd = fd as usize;
        if fd >= FD_SETSIZE {
            return false;
        }
        self.fds_bits[fd / USIZE_BITS] |= 1 << (fd % USIZE_BITS);
        true
    }

    /// Equivalent to FD_CLR.
    pub fn unset(&mut self, fd: u32) -> bool {
        let fd = fd as usize;
        if fd >= FD_SETSIZE {
            return false;
        }
        self.fds_bits[fd / USIZE_BITS] &= !(1 << (fd % USIZE_BITS));
        true
    }

    /// Equivalent to FD_ISSET.
    pub fn is_set(&self, fd: u32) -> bool {
        let fd = fd as usize;
        if fd >= FD_SETSIZE {
            return false;
        }
        (self.fds_bits[fd / USIZE_BITS] & (1 << (fd % USIZE_BITS))) != 0
    }

    /// Equivalent to FD_ZERO.
    pub fn clear(&mut self) {
        for slot in self.fds_bits.iter_mut() {
            *slot = 0;
        }
    }
}
