#[cfg(target_arch = "x86_64")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PtrT(u32);

#[cfg(not(target_arch = "x86_64"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PtrT(u64);

impl PtrT {
    pub fn new(ptr: usize) -> Self {
        PtrT(
            ptr.try_into()
                .expect("Pointer truncated when converting to ptr_t"),
        )
    }

    pub fn addr(self) -> usize {
        self.0 as usize
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}
