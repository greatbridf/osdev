use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct OpenFlags: u32 {
        /// Open for writing only
        const O_WRONLY = 0x1;
        /// Open for reading and writing
        const O_RDWR = 0x2;
        /// Create file if it does not exist
        const O_CREAT = 0x40;
        /// Exclusive access, fail if file exists
        const O_EXCL = 0x80;
        /// Truncate file to zero length if it exists
        const O_TRUNC = 0x200;
        /// Open file in append mode
        const O_APPEND = 0x400;
        /// Non-blocking mode
        const O_NONBLOCK = 0x800;
        /// Open directory
        const O_DIRECTORY = 0x10000;
        /// Do not follow symbolic links
        const O_NOFOLLOW = 0x20000;
        /// Close on exec
        const O_CLOEXEC = 0x80000;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct FDFlags: u32 {
        /// Close on exec
        const FD_CLOEXEC = 0x1;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct AtFlags: u32 {
        /// Do not follow symbolic links
        const AT_SYMLINK_NOFOLLOW = 0x100;
        /// Allow removal of directories
        const AT_REMOVEDIR = 0x200;
        /// Follow symbolic links when resolving paths
        const AT_SYMLINK_FOLLOW = 0x400;
        /// Use the file descriptor with empty path
        const AT_EMPTY_PATH = 0x1000;
        /// Force synchronization of file attributes
        const AT_STATX_FORCE_SYNC = 0x2000;
        /// Do not synchronize file attributes
        const AT_STATX_DONT_SYNC = 0x4000;
    }
}

impl FDFlags {
    pub fn close_on_exec(&self) -> bool {
        self.contains(FDFlags::FD_CLOEXEC)
    }
}

impl OpenFlags {
    pub fn as_fd_flags(&self) -> FDFlags {
        if self.contains(OpenFlags::O_CLOEXEC) {
            FDFlags::FD_CLOEXEC
        } else {
            FDFlags::empty()
        }
    }

    pub fn read(&self) -> bool {
        !self.contains(Self::O_WRONLY)
    }

    pub fn write(&self) -> bool {
        self.intersects(Self::O_WRONLY | Self::O_RDWR)
    }

    pub fn append(&self) -> bool {
        self.contains(Self::O_APPEND)
    }

    pub fn directory(&self) -> bool {
        self.contains(Self::O_DIRECTORY)
    }

    pub fn truncate(&self) -> bool {
        self.contains(Self::O_TRUNC)
    }

    pub fn follow_symlink(&self) -> bool {
        !self.contains(Self::O_NOFOLLOW)
    }

    pub fn as_rwa(&self) -> (bool, bool, bool) {
        (self.read(), self.write(), self.append())
    }
}

impl AtFlags {
    pub fn at_empty_path(&self) -> bool {
        self.contains(AtFlags::AT_EMPTY_PATH)
    }

    /// # Notice
    /// `no_follow` and `follow` are **DIFFERENT** and are used in different contexts.
    ///
    /// `follow` is used to reverse the default behavior of `linkat`, which does not
    /// follow symlinks by default.
    pub fn no_follow(&self) -> bool {
        self.contains(AtFlags::AT_SYMLINK_NOFOLLOW)
    }

    /// # Notice
    /// `no_follow` and `follow` are **DIFFERENT** and are used in different contexts.
    ///
    /// `follow` is used to reverse the default behavior of `linkat`, which does not
    /// follow symlinks by default.
    pub fn follow(&self) -> bool {
        self.contains(AtFlags::AT_SYMLINK_FOLLOW)
    }

    pub fn statx_default_sync(&self) -> bool {
        !self.intersects(AtFlags::AT_STATX_FORCE_SYNC | AtFlags::AT_STATX_DONT_SYNC)
    }
}
