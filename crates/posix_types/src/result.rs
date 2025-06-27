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
