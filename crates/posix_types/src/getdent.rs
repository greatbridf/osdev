#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct UserDirent64 {
    /// Inode number
    pub d_ino: u64,
    /// Implementation defined. We ignore it
    pub d_off: u64,
    /// Length of this record
    pub d_reclen: u16,
    /// File type. Set to 0
    pub d_type: u8,
    /// Filename with a padding '\0'
    pub d_name: [u8; 0],
}

/// File type is at offset `d_reclen - 1`. Set it to 0
#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
pub struct UserDirent {
    /// Inode number
    pub d_ino: u32,
    /// Implementation defined. We ignore it
    pub d_off: u32,
    /// Length of this record
    pub d_reclen: u16,
    /// Filename with a padding '\0'
    pub d_name: [u8; 0],
}
