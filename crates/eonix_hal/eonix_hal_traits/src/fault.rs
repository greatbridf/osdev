use bitflags::bitflags;

bitflags! {
    #[derive(Debug)]
    pub struct PageFaultErrorCode: u32 {
        const NonPresent = 1;
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
    PageFault(PageFaultErrorCode),
    Unknown(usize),
}
