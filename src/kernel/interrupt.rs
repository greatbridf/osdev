use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use lazy_static::lazy_static;

use crate::bindings::root::EINVAL;
use crate::Spin;

lazy_static! {
    static ref IRQ_HANDLERS: Spin<[Vec<Box<dyn Fn() + Send>>; 16]> =
        Spin::new(core::array::from_fn(|_| vec![]));
}

#[no_mangle]
pub extern "C" fn irq_handler_rust(irqno: core::ffi::c_int) {
    assert!(irqno >= 0 && irqno < 16);

    let handlers = IRQ_HANDLERS.lock();

    for handler in handlers[irqno as usize].iter() {
        handler();
    }
}

pub fn register_irq_handler<F>(irqno: i32, handler: F) -> Result<(), u32>
where
    F: Fn() + Send + 'static,
{
    if irqno < 0 || irqno >= 16 {
        return Err(EINVAL);
    }

    IRQ_HANDLERS.lock_irq()[irqno as usize].push(Box::new(handler));
    Ok(())
}
