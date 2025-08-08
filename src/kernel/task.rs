mod clone;
mod futex;
mod kernel_stack;
mod loader;
mod process;
mod process_group;
mod process_list;
mod session;
mod signal;
mod thread;

pub use clone::{do_clone, CloneArgs, CloneFlags};
pub use futex::{futex_wait, futex_wake, parse_futexop, FutexFlags, FutexOp, RobustListHead};
pub use kernel_stack::KernelStack;
pub use loader::ProgramLoader;
pub use process::{alloc_pid, Process, ProcessBuilder, WaitId, WaitObject, WaitType};
pub use process_group::ProcessGroup;
pub use process_list::ProcessList;
pub use session::Session;
pub use signal::SignalAction;
pub use thread::{yield_now, Thread, ThreadBuilder};

fn do_block_on<F>(mut future: core::pin::Pin<&mut F>) -> F::Output
where
    F: core::future::Future,
{
    let waker = core::task::Waker::noop();
    let mut cx = core::task::Context::from_waker(&waker);

    loop {
        match future.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(output) => return output,
            core::task::Poll::Pending => {}
        }
    }
}

/// Constantly poll the given future until it is ready, blocking the current thread.
///
/// # Warning
/// This function will block the current thread and should not be used in async
/// contexts as it might cause infinite blocking or deadlocks. The following is
/// a bad example:
///
/// ```ignore
/// block_on(async {
///     // This will block the current thread forever.
///     loop {
///         println_debug!("This will never end!");
///     }
/// });
///
/// // The code below will never be reached.
/// println_debug!("You'll never see this message!");
/// ```
///
/// Use [`stackful`] instead to run async (or computational) code in a separate
/// stackful (and preemptive) context or `RUNTIME.spawn` to run async code in
/// the runtime's executor.
pub fn block_on<F>(future: F) -> F::Output
where
    F: core::future::Future,
{
    do_block_on(core::pin::pin!(future))
}

/// Run the given future in a stackful context, allowing it to be preempted by
/// timer interrupts.
///
/// ```ignore
/// RUNTIME.spawn(stackful(async {
///     // Some simulated computation heavy task.
///     loop {
///         println_debug!("Hello from stackful future!");
///     }
/// }));
/// ```
pub async fn stackful<F>(mut future: F) -> F::Output
where
    F: core::future::Future,
{
    use core::cell::UnsafeCell;
    use eonix_hal::traits::fault::Fault;
    use eonix_hal::traits::trap::RawTrapContext;
    use eonix_hal::traits::trap::TrapReturn;
    use eonix_hal::trap::TrapContext;
    use eonix_log::println_debug;
    use eonix_runtime::executor::Stack;

    use crate::kernel::{
        interrupt::{default_fault_handler, default_irq_handler},
        timer::{should_reschedule, timer_interrupt},
    };

    let stack = KernelStack::new();

    fn execute<F>(
        future: core::pin::Pin<&mut F>,
        output_ptr: core::ptr::NonNull<Option<F::Output>>,
    ) -> !
    where
        F: core::future::Future,
    {
        let output = do_block_on(future);

        unsafe {
            output_ptr.write(Some(output));
        }

        unsafe {
            core::arch::asm!("ebreak");
        }

        unreachable!()
    }

    let sp = stack.get_bottom();
    let output = UnsafeCell::new(None);

    let mut trap_ctx = TrapContext::new();

    trap_ctx.set_user_mode(false);
    trap_ctx.set_interrupt_enabled(true);
    let _ = trap_ctx.set_user_call_frame(
        execute::<F> as usize,
        Some(sp.addr().get()),
        None,
        &[(&raw mut future) as usize, output.get() as usize],
        |_, _| Ok::<(), u32>(()),
    );

    loop {
        unsafe {
            trap_ctx.trap_return();
        }

        match trap_ctx.trap_type() {
            eonix_hal::traits::trap::TrapType::Syscall { .. } => {}
            eonix_hal::traits::trap::TrapType::Fault(fault) => {
                // Breakpoint
                if let Fault::Unknown(3) = &fault {
                    println_debug!("Breakpoint hit, returning output");
                    break output.into_inner().unwrap();
                }

                default_fault_handler(fault, &mut trap_ctx)
            }
            eonix_hal::traits::trap::TrapType::Irq { callback } => callback(default_irq_handler),
            eonix_hal::traits::trap::TrapType::Timer { callback } => {
                callback(timer_interrupt);

                if should_reschedule() {
                    yield_now().await;
                }
            }
        }
    }
}
