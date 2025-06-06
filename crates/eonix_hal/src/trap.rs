use eonix_hal_traits::trap::IsRawTrapContext;

pub use crate::arch::trap::{disable_irqs, disable_irqs_save, enable_irqs, IrqState, TrapContext};

struct _CheckTrapContext(IsRawTrapContext<TrapContext>);
