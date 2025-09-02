use crate::kernel::{
    constants::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG},
    syscall::{FromSyscallArg, SyscallRetVal},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Mode(u32);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    REG,
    DIR,
    LNK,
    BLK,
    CHR,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Permission(u32);

impl Mode {
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn is_blk(&self) -> bool {
        (self.0 & S_IFMT) == S_IFBLK
    }

    pub const fn is_chr(&self) -> bool {
        (self.0 & S_IFMT) == S_IFCHR
    }

    pub const fn bits(&self) -> u32 {
        self.0
    }

    pub const fn format_bits(&self) -> u32 {
        self.0 & S_IFMT
    }

    pub const fn non_format_bits(&self) -> u32 {
        self.0 & !S_IFMT
    }

    pub fn format(&self) -> Format {
        match self.format_bits() {
            S_IFREG => Format::REG,
            S_IFDIR => Format::DIR,
            S_IFLNK => Format::LNK,
            S_IFBLK => Format::BLK,
            S_IFCHR => Format::CHR,
            _ => panic!("unknown format bits: {:#o}", self.format_bits()),
        }
    }

    pub fn perm(&self) -> Permission {
        Permission::new(self.non_format_bits())
    }

    pub const fn non_format(&self) -> Self {
        Self::new(self.non_format_bits())
    }

    pub const fn set_perm(&mut self, perm: Permission) {
        self.0 = self.format_bits() | perm.bits();
    }
}

impl Format {
    pub const fn as_raw(&self) -> u32 {
        match self {
            Self::REG => S_IFREG,
            Self::DIR => S_IFDIR,
            Self::LNK => S_IFLNK,
            Self::BLK => S_IFBLK,
            Self::CHR => S_IFCHR,
        }
    }
}

impl Permission {
    const RWX: [&str; 8] = ["---", "--x", "-w-", "-wx", "r--", "r-x", "rw-", "rwx"];

    pub const fn new(perm_bits: u32) -> Self {
        Self(perm_bits & 0o7777)
    }

    pub const fn bits(&self) -> u32 {
        self.0
    }

    pub const fn mask_with(&self, mask: Self) -> Self {
        Self(self.0 & mask.0)
    }
}

impl core::fmt::Debug for Mode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.non_format_bits() & !0o777 {
            0 => write!(
                f,
                "Mode({format:?}, {perm:#o})",
                format = self.format(),
                perm = self.non_format_bits()
            )?,
            rem => write!(
                f,
                "Mode({format:?}, {perm:#o}, rem={rem:#x})",
                format = self.format(),
                perm = self.non_format_bits() & 0o777
            )?,
        }

        Ok(())
    }
}

impl core::fmt::Debug for Format {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::REG => write!(f, "REG"),
            Self::DIR => write!(f, "DIR"),
            Self::LNK => write!(f, "LNK"),
            Self::BLK => write!(f, "BLK"),
            Self::CHR => write!(f, "CHR"),
        }
    }
}

impl core::fmt::Debug for Permission {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let owner = self.0 >> 6 & 0o7;
        let group = self.0 >> 3 & 0o7;
        let other = self.0 & 0o7;

        write!(
            f,
            "{}{}{}",
            Self::RWX[owner as usize],
            Self::RWX[group as usize],
            Self::RWX[other as usize]
        )
    }
}

impl FromSyscallArg for Mode {
    fn from_arg(value: usize) -> Self {
        Mode::new(value as u32)
    }
}

impl SyscallRetVal for Mode {
    fn into_retval(self) -> Option<usize> {
        Some(self.bits() as usize)
    }
}

impl FromSyscallArg for Permission {
    fn from_arg(value: usize) -> Self {
        Permission::new(value as u32)
    }
}

impl SyscallRetVal for Permission {
    fn into_retval(self) -> Option<usize> {
        Some(self.bits() as usize)
    }
}
