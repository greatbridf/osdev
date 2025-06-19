use bitflags::bitflags;
use eonix_mm::address::VAddr;

bitflags! {
    #[derive(Debug)]
    pub struct PageFaultErrorCode: u32 {
        const Read = 2;
        const Write = 4;
        const InstructionFetch = 8;
        const UserAccess = 16;
    }
}

#[derive(Debug)]
pub enum Fault {
    InvalidOp,
    BadAccess,
    PageFault {
        error_code: PageFaultErrorCode,
        address: VAddr,
    },
    Unknown(usize),
}
