use core::marker::PhantomData;

use eonix_mm::address::VAddr;

use crate::fault::Fault;

pub trait Stack {
    fn get_bottom(&self) -> *mut usize;
}

/// A raw trap context.
///
/// This should be implemented by the architecture-specific trap context
/// and will be used in the HAL crates.
#[doc(notable_trait)]
pub trait RawTrapContext: Copy {
    type FIrq: FnOnce(fn(irqno: usize));
    type FTimer: FnOnce(fn());

    /// **Don't use this function unless you know what you're doing**
    ///
    /// Create a blank trap context.
    ///
    /// The context should be in a state that is ready to be used but whether
    /// the interrupt is enabled or the context is in user mode is unspecified.
    fn blank() -> Self;

    /// Create a new trap context.
    ///
    /// The context will be in a state that is ready to be used. Whether the
    /// interrupt is enabled or the context is in user mode is specified by
    /// the arguments.
    fn new(int_enabled: bool, user: bool) -> Self {
        let mut me = Self::blank();
        me.set_interrupt_enabled(int_enabled);
        me.set_user_mode(user);

        me
    }

    fn trap_type(&self) -> TrapType<Self::FIrq, Self::FTimer>;

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
        &mut self, pc: usize, sp: Option<usize>, ra: Option<usize>,
        args: &[usize], write_memory: impl Fn(VAddr, &[u8]) -> Result<(), E>,
    ) -> Result<(), E>;

    fn set_kernel_call_frame(
        &mut self, pc: usize, stack: &impl Stack, ra: Option<usize>,
        args: &[usize],
    );
}

#[doc(notable_trait)]
pub trait TrapReturn {
    /// Return to the context before the trap occurred.
    ///
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that the
    /// context before the trap is valid, that is, that the stack pointer
    /// points to a valid stack frame and the program counter points to some
    /// valid instruction.
    unsafe fn trap_return(&mut self);

    /// Switch to the context before the trap occurred.
    /// This function will NOT capture traps and will never return.
    ///
    /// # Safety
    /// This function is unsafe because the caller MUST ensure that the
    /// context before the trap is valid, that is, that the stack pointer
    /// points to a valid stack frame and the program counter points to some
    /// valid instruction. Besides, the caller MUST ensure that all variables
    /// in the current context are released.
    unsafe fn trap_return_noreturn(&mut self) -> !;
}

pub trait IrqState {
    /// Restore the IRQ state.
    fn restore(self);
}

/// The reason that caused the trap.
pub enum TrapType<FIrq, FTimer>
where
    FIrq: FnOnce(fn(irqno: usize)),
    FTimer: FnOnce(fn()),
{
    Syscall { no: usize, args: [usize; 6] },
    Fault(Fault),
    Breakpoint,
    Irq { callback: FIrq },
    Timer { callback: FTimer },
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
