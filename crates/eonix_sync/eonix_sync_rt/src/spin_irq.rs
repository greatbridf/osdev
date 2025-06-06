use eonix_hal::traits::trap::IrqState as _;
use eonix_hal::trap::{disable_irqs_save, IrqState};
use eonix_spin::{ContextUnlock, Spin, SpinContext, SpinGuard, UnlockedContext};
use eonix_sync_base::Relax;

pub struct IrqContext(IrqState);

pub struct UnlockedIrqContext(IrqState);

pub trait SpinIrq {
    type Value: ?Sized;
    type Context: SpinContext;
    type Relax;

    fn lock_irq(&self) -> SpinGuard<Self::Value, Self::Context, Self::Relax>;
}

impl SpinContext for IrqContext {
    fn save() -> Self {
        IrqContext(disable_irqs_save())
    }

    fn restore(self) {
        self.0.restore();
    }
}

impl ContextUnlock for IrqContext {
    type Unlocked = UnlockedIrqContext;

    fn unlock(self) -> Self::Unlocked {
        UnlockedIrqContext(self.0)
    }
}

impl UnlockedContext for UnlockedIrqContext {
    type Relocked = IrqContext;

    fn relock(self) -> Self::Relocked {
        IrqContext(self.0)
    }
}

impl<T, R> SpinIrq for Spin<T, R>
where
    T: ?Sized,
    R: Relax,
{
    type Value = T;
    type Context = IrqContext;
    type Relax = R;

    fn lock_irq(&self) -> SpinGuard<Self::Value, Self::Context, Self::Relax> {
        self.lock_with_context(IrqContext::save())
    }
}
