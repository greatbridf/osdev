use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct RenameFlags: u32 {
        /// Do not overwrite existing files
        const RENAME_NOREPLACE = 0x1;
        /// Exchange the names of two files
        const RENAME_EXCHANGE = 0x2;
    }
}
