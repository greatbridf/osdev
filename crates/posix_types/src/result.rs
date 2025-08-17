pub enum PosixError {
    EFAULT = 14,
    EXDEV = 18,
    EINVAL = 22,
}

impl From<PosixError> for u32 {
    fn from(error: PosixError) -> Self {
        match error {
            PosixError::EFAULT => 14,
            PosixError::EXDEV => 18,
            PosixError::EINVAL => 22,
        }
    }
}

impl core::fmt::Debug for PosixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EFAULT => write!(f, "EFAULT"),
            Self::EXDEV => write!(f, "EXDEV"),
            Self::EINVAL => write!(f, "EINVAL"),
        }
    }
}
