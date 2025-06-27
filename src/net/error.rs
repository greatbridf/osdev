#[derive(Clone, Copy)]
pub enum NetError {
    SystemError(u32, &'static str),
    IoFailure,
    DeviceBusy,
    Unsupported,
    AlreadyRegistered,
}
