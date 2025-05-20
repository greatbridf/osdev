use eonix_mm::address::{VAddr, VRange};

const USER_SPACE_MEMORY_TOP: VAddr = VAddr::from(0x8000_0000_0000);
const KERNEL_SPACE_MEMORY_BOTTOM: VAddr = VAddr::from(0xffff_8000_0000_0000);

pub trait VAddrExt {
    fn is_user(&self) -> bool;
}

pub trait VRangeExt {
    #[allow(dead_code)]
    fn is_kernel(&self) -> bool;
    fn is_user(&self) -> bool;
}

impl VAddrExt for VAddr {
    fn is_user(&self) -> bool {
        (..USER_SPACE_MEMORY_TOP).contains(&self)
    }
}

impl VRangeExt for VRange {
    fn is_user(&self) -> bool {
        !(self.end() > USER_SPACE_MEMORY_TOP || self.start() >= USER_SPACE_MEMORY_TOP)
    }

    fn is_kernel(&self) -> bool {
        self.start() >= KERNEL_SPACE_MEMORY_BOTTOM
    }
}
