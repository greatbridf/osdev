use crate::{context::RawTaskContext, fault::Fault};
use core::marker::PhantomData;
use eonix_mm::address::VAddr;

/// A raw trap context.
///
/// This should be implemented by the architecture-specific trap context
/// and will be used in the HAL crates.
#[doc(notable_trait)]
pub trait RawTrapContext: Copy {
    fn new() -> Self;

    fn trap_type(&self) -> TrapType;

    fn get_program_counter(&self) -> usize;
    fn get_stack_pointer(&self) -> usize;

    fn set_program_counter(&mut self, pc: usize);
    fn set_stack_pointer(&mut self, sp: usize);

    fn is_interrupt_enabled(&self) -> bool;
    fn set_interrupt_enabled(&mut self, enabled: bool);

    fn is_user_mode(&self) -> bool;
    fn set_user_mode(&mut self, user: bool);

    fn set_user_return_value(&mut self, retval: usize);

    fn set_user_call_frame<E>(
        &mut self,
        pc: usize,
        sp: Option<usize>,
        ra: Option<usize>,
        args: &[usize],
        write_memory: impl Fn(VAddr, &[u8]) -> Result<(), E>,
    ) -> Result<(), E>;
}

#[doc(notable_trait)]
pub trait TrapReturn {
    type TaskContext: RawTaskContext;

    /// Return to the context before the trap occurred.
    ///
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that the
    /// context before the trap is valid, that is, that the stack pointer
    /// points to a valid stack frame and the program counter points to some
    /// valid instruction.
    unsafe fn trap_return(&mut self);
}

pub trait IrqState {
    /// Restore the IRQ state.
    fn restore(self);
}

/// The reason that caused the trap.
pub enum TrapType {
    Syscall { no: usize, args: [usize; 6] },
    Fault(Fault),
    Irq(usize),
    Timer,
}

/// A marker type that indicates that the type is a raw trap context.
///
/// # Usage
///
/// Check whether a type implements `RawTrapContext` using a `PhantomData` field.
///
/// The following code should fail to compile:
///
/// ```compile_fail
/// # use eonix_hal_traits::trap::IsRawTrapContext;
/// struct NonRawTrapContext; // Does not implement `RawTrapContext`!
///
/// // Compile-time error: `NonRawTrapContext` does not implement `RawTrapContext`.
/// struct UserStruct(NonRawTrapContext, IsRawTrapContext<NonRawTrapContext>);
/// ```
///
/// While the following code should compile:
///
/// ```no_run
/// # use eonix_hal_traits::trap::IsRawTrapContext;
/// struct RawTrapContextType;
///
/// impl RawTrapContext for RawTrapContextType {
///     // ...
/// #   fn new() -> Self { todo!() }
/// #   fn trap_type() -> TrapType { todo!() }
/// #   fn get_program_counter(&self) -> usize { todo!() }
/// #   fn get_stack_pointer(&self) -> usize { todo!() }
/// #   fn set_program_counter(&mut self, _: usize) { todo!() }
/// #   fn set_stack_pointer(&mut self, _: usize) { todo!() }
/// #   fn is_interrupt_enabled(&self) -> bool { todo!() }
/// #   fn set_interrupt_enabled(&mut self, _: bool) { todo!() }
/// #   fn is_user_mode(&self) -> bool { todo!() }
/// #   fn set_user_mode(&mut self, _: bool) { todo!() }
/// #   fn set_user_return_value(&mut self, _: usize) { todo!() }
/// }
///
/// struct UserStruct(RawTrapContextType, IsRawTrapContext<RawTrapContextType>);
/// ```
pub struct IsRawTrapContext<T>(PhantomData<T>)
where
    T: RawTrapContext;
