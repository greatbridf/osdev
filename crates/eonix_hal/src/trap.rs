use eonix_hal_traits::trap::IsRawTrapContext;

pub use crate::arch::TrapContext;

// TODO: Remove this once the arch module is fully implemented.
pub use crate::arch::TRAP_STUBS_START;

struct _CheckTrapContext(IsRawTrapContext<TrapContext>);
