use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::bindings::root::EINVAL;

static mut IRQ_HANDLERS: spin::Mutex<[Option<Vec<Box<dyn Fn()>>>; 16]> =
    spin::Mutex::new([const { None }; 16]);

#[no_mangle]
pub extern "C" fn irq_handler_rust(irqno: core::ffi::c_int) {
    assert!(irqno >= 0 && irqno < 16);

    let handlers = unsafe { IRQ_HANDLERS.lock() };

    match handlers[irqno as usize] {
        Some(ref handlers) => {
            for handler in handlers {
                handler();
            }
        }
        None => {}
    }
}

pub fn register_irq_handler<F>(irqno: i32, handler: F) -> Result<(), u32>
where
    F: Fn() + 'static,
{
    if irqno < 0 || irqno >= 16 {
        return Err(EINVAL);
    }

    let mut handlers = unsafe { IRQ_HANDLERS.lock() };

    match handlers[irqno as usize] {
        Some(ref mut handlers) => handlers.push(Box::new(handler)),
        None => {
            handlers[irqno as usize].replace(vec![Box::new(handler)]);
        }
    }

    Ok(())
}
