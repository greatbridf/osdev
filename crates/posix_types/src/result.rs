pub enum PosixError {
    ENOENT = 2,
    EFAULT = 14,
    EXDEV = 18,
    ENOTDIR = 20,
    EINVAL = 22,
}

impl From<PosixError> for u32 {
    fn from(error: PosixError) -> Self {
        match error {
            PosixError::ENOENT => 2,
            PosixError::EFAULT => 14,
            PosixError::EXDEV => 18,
            PosixError::ENOTDIR => 20,
            PosixError::EINVAL => 22,
        }
    }
}

impl core::fmt::Debug for PosixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ENOENT => write!(f, "ENOENT"),
            Self::EFAULT => write!(f, "EFAULT"),
            Self::EXDEV => write!(f, "EXDEV"),
            Self::ENOTDIR => write!(f, "ENOTDIR"),
            Self::EINVAL => write!(f, "EINVAL"),
        }
    }
}
