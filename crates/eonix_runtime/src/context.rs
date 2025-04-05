use core::{cell::UnsafeCell, mem::transmute};

#[derive(Debug)]
pub struct ExecutionContext(UnsafeCell<arch::TaskContext>);

unsafe impl Sync for ExecutionContext {}

impl ExecutionContext {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(arch::TaskContext::new()))
    }

    pub fn set_ip(&mut self, ip: usize) {
        let Self(context) = self;
        context.get_mut().ip(ip);
    }

    pub fn set_sp(&mut self, sp: usize) {
        let Self(context) = self;
        context.get_mut().sp(sp);
    }

    pub fn set_interrupt(&mut self, is_enabled: bool) {
        let Self(context) = self;
        context.get_mut().interrupt(is_enabled);
    }

    pub fn call1<T>(&mut self, func: unsafe extern "C" fn(T) -> !, arg: usize) {
        let Self(context) = self;
        context
            .get_mut()
            .call1(unsafe { transmute(func as *mut ()) }, [arg]);
    }

    pub fn switch_to(&self, to: &Self) {
        let Self(from_ctx) = self;
        let Self(to_ctx) = to;
        unsafe {
            arch::TaskContext::switch(&mut *from_ctx.get(), &mut *to_ctx.get());
        }
    }

    pub fn switch_noreturn(&self) -> ! {
        let mut from_ctx = arch::TaskContext::new();
        let Self(to_ctx) = self;
        unsafe {
            arch::TaskContext::switch(&mut from_ctx, &mut *to_ctx.get());
        }
        unreachable!("We should never return from switch_to_noreturn");
    }
}
