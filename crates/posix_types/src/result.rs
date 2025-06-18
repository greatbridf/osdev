pub enum PosixError {
    EFAULT = 14,
    EINVAL = 22,
}

impl From<PosixError> for u32 {
    fn from(error: PosixError) -> Self {
        match error {
            PosixError::EFAULT => 14,
            PosixError::EINVAL => 22,
        }
    }
}
