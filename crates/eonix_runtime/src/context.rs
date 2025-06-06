use core::{cell::UnsafeCell, mem::transmute};
use eonix_hal::context::TaskContext;
use eonix_hal::traits::context::RawTaskContext;

#[derive(Debug)]
pub struct ExecutionContext(UnsafeCell<TaskContext>);

unsafe impl Sync for ExecutionContext {}

impl ExecutionContext {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(TaskContext::new()))
    }

    pub fn set_ip(&mut self, ip: usize) {
        let Self(context) = self;
        context.get_mut().set_program_counter(ip);
    }

    pub fn set_sp(&mut self, sp: usize) {
        let Self(context) = self;
        context.get_mut().set_stack_pointer(sp);
    }

    pub fn set_interrupt(&mut self, is_enabled: bool) {
        let Self(context) = self;
        context.get_mut().set_interrupt_enabled(is_enabled);
    }

    pub fn call1<T>(&mut self, func: unsafe extern "C" fn(T) -> !, arg: usize) {
        let Self(context) = self;
        context
            .get_mut()
            .call(unsafe { transmute(func as *mut ()) }, arg);
    }

    pub fn switch_to(&self, to: &Self) {
        let Self(from_ctx) = self;
        let Self(to_ctx) = to;
        unsafe {
            TaskContext::switch(&mut *from_ctx.get(), &mut *to_ctx.get());
        }
    }

    pub fn switch_noreturn(&self) -> ! {
        let Self(to_ctx) = self;
        unsafe {
            TaskContext::switch_to_noreturn(&mut *to_ctx.get());
        }
    }
}
