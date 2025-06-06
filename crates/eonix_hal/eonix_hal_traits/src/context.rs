#[doc(notable_trait)]
pub trait RawTaskContext: Sized {
    /// Creates a new instance of the task context with interrupt enabled and the program
    /// counter and stack pointer set to zero (a.k.a. some invalid state).
    ///
    /// Using the created context without setting the program counter and stack pointer
    /// will result in undefined behavior.
    fn new() -> Self;

    fn set_program_counter(&mut self, pc: usize);
    fn set_stack_pointer(&mut self, sp: usize);

    fn is_interrupt_enabled(&self) -> bool;
    fn set_interrupt_enabled(&mut self, is_enabled: bool);

    /// Sets the instruction pointer to the given function and prepares the context
    /// to call it with the given argument.
    fn call(&mut self, func: unsafe extern "C" fn(usize) -> !, arg: usize);

    /// Switch the execution context from `from` to `to`.
    ///
    /// # Safety
    /// This function is unsafe because it performs a context switch, which can lead to
    /// undefined behavior if the contexts are not properly set up.
    unsafe extern "C" fn switch(from: &mut Self, to: &mut Self);

    /// Switches the execution context to `to` where we will not return.
    ///
    /// # Safety
    /// This function is unsafe because it performs a context switch that does not return.
    /// The caller must ensure that the `to` context is properly set up and that it will not
    /// return to the caller.
    unsafe extern "C" fn switch_to_noreturn(to: &mut Self) -> ! {
        let mut from_ctx = Self::new();
        unsafe {
            Self::switch(&mut from_ctx, to);
        }

        unreachable!("We should never return from `switch_to_noreturn()`");
    }
}
