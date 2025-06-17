#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptorFlags(u32);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptor {
    pub entry: u32,
    pub base: u32,
    pub limit: u32,
    pub flags: UserDescriptorFlags,
}

#[allow(dead_code)]
impl UserDescriptorFlags {
    pub fn is_32bit_segment(&self) -> bool {
        self.0 & 0b1 != 0
    }

    pub fn contents(&self) -> u32 {
        self.0 & 0b110
    }

    pub fn is_read_exec_only(&self) -> bool {
        self.0 & 0b1000 != 0
    }

    pub fn is_limit_in_pages(&self) -> bool {
        self.0 & 0b10000 != 0
    }

    pub fn is_present(&self) -> bool {
        self.0 & 0b100000 == 0
    }

    pub fn is_usable(&self) -> bool {
        self.0 & 0b1000000 != 0
    }
}
