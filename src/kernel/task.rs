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
mod user_tls;

pub use clone::{do_clone, CloneArgs, CloneFlags};
use eonix_hal::symbol_addr;
pub use futex::{
    futex_exec, futex_exit, futex_wait, futex_wake, parse_futexop, FutexFlags,
    FutexOp, RobustListHead,
};
pub use kernel_stack::KernelStack;
pub use loader::ProgramLoader;
pub use process::{
    alloc_pid, Process, ProcessBuilder, WaitId, WaitObject, WaitType,
};
pub use process_group::ProcessGroup;
pub use process_list::ProcessList;
pub use session::Session;
pub use signal::SignalAction;
pub use thread::{yield_now, Thread, ThreadAlloc, ThreadBuilder};
pub use user_tls::{UserTLS, UserTLSDescriptor};

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
    use alloc::sync::Arc;
    use alloc::task::Wake;
    use core::cell::UnsafeCell;
    use core::future::Future;
    use core::pin::Pin;
    use core::ptr::NonNull;
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::task::{Context, Poll, Waker};

    use eonix_hal::traits::trap::{RawTrapContext, TrapReturn, TrapType};
    use eonix_hal::trap::TrapContext;
    use eonix_preempt::assert_preempt_enabled;
    use eonix_runtime::executor::Stack;
    use eonix_runtime::task::Task;
    use thread::wait_for_wakeups;

    use crate::kernel::interrupt::{
        default_fault_handler, default_irq_handler,
    };
    use crate::kernel::timer::{should_reschedule, timer_interrupt};

    let stack = KernelStack::new();

    fn execute<F>(
        mut future: Pin<&mut F>, output_ptr: NonNull<Option<F::Output>>,
    ) -> !
    where
        F: Future,
    {
        struct WakeSaver {
            task: Arc<Task>,
            woken: AtomicBool,
        }

        impl Wake for WakeSaver {
            fn wake_by_ref(self: &Arc<Self>) {
                // SAFETY: If we read true below in the loop, we must have been
                //         woken up and acquired our waker's work by the runtime.
                self.woken.store(true, Ordering::Relaxed);
                self.task.wake_by_ref();
            }

            fn wake(self: Arc<Self>) {
                self.wake_by_ref();
            }
        }

        let wake_saver = Arc::new(WakeSaver {
            task: Task::current().clone(),
            woken: AtomicBool::new(false),
        });
        let waker = Waker::from(wake_saver.clone());
        let mut cx = Context::from_waker(&waker);

        let output = loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(output) => break output,
                Poll::Pending => {
                    assert_preempt_enabled!(
                        "Blocking in stackful futures is not allowed."
                    );

                    if Task::current().is_ready() {
                        continue;
                    }

                    // SAFETY: The runtime must have ensured that we can see the
                    //         work done by the waker.
                    if wake_saver.woken.swap(false, Ordering::Relaxed) {
                        continue;
                    }

                    unsafe {
                        #[cfg(target_arch = "riscv64")]
                        core::arch::asm!("ebreak");

                        #[cfg(target_arch = "loongarch64")]
                        core::arch::asm!("break 1");
                    }
                }
            }
        };

        drop(cx);
        drop(waker);
        drop(wake_saver);

        unsafe {
            output_ptr.write(Some(output));
        }

        unsafe {
            #[cfg(target_arch = "riscv64")]
            core::arch::asm!("ebreak");

            #[cfg(target_arch = "loongarch64")]
            core::arch::asm!("break 1");
        }

        unreachable!()
    }

    let sp = stack.get_bottom();
    let mut output = UnsafeCell::new(None);

    let mut trap_ctx = TrapContext::new();

    trap_ctx.set_user_mode(false);
    trap_ctx.set_interrupt_enabled(true);
    let _ = trap_ctx.set_user_call_frame(
        symbol_addr!(execute::<F>),
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
            TrapType::Syscall { .. } => {}
            TrapType::Fault(fault) => {
                default_fault_handler(fault, &mut trap_ctx)
            }
            TrapType::Irq { callback } => callback(default_irq_handler),
            TrapType::Timer { callback } => {
                callback(timer_interrupt);

                if eonix_preempt::count() == 0 && should_reschedule() {
                    yield_now().await;
                }
            }
            TrapType::Breakpoint => {
                if let Some(output) = output.get_mut().take() {
                    break output;
                } else {
                    wait_for_wakeups().await;
                }

                #[cfg(target_arch = "riscv64")]
                trap_ctx
                    .set_program_counter(trap_ctx.get_program_counter() + 2);

                #[cfg(target_arch = "loongarch64")]
                trap_ctx
                    .set_program_counter(trap_ctx.get_program_counter() + 4);
            }
        }
    }
}
