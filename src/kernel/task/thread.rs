use super::{
    signal::{RaiseResult, Signal, SignalList},
    Process, ProcessList,
};
use crate::{
    kernel::{
        interrupt::default_irq_handler,
        syscall::{syscall_handlers, SyscallHandler},
        task::{clone::CloneArgs, CloneFlags},
        timer::timer_interrupt,
        user::{dataflow::CheckedUserPointer, UserPointerMut},
        vfs::{filearray::FileArray, FsContext},
    },
    prelude::*,
};
use alloc::sync::Arc;
use arch::FpuState;
use atomic_unique_refcell::AtomicUniqueRefCell;
use core::{
    future::Future,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll, Waker},
};
use eonix_hal::traits::trap::IrqState;
use eonix_hal::{
    processor::{UserTLS, CPU},
    traits::{
        fault::Fault,
        fpu::RawFpuState as _,
        trap::{RawTrapContext, TrapReturn, TrapType},
    },
    trap::{disable_irqs_save, TrapContext},
};
use eonix_mm::address::{Addr as _, VAddr};
use eonix_runtime::run::{Contexted, Run, RunState};
use eonix_sync::AsProofMut as _;
use pointers::BorrowedArc;
use posix_types::x86_64::UserDescriptor;

#[eonix_percpu::define_percpu]
static CURRENT_THREAD: Option<NonNull<Thread>> = None;

pub struct ThreadRunnable<F: Future> {
    thread: Arc<Thread>,
    future: F,
}

pub struct ThreadBuilder {
    tid: Option<u32>,
    name: Option<Arc<[u8]>>,
    process: Option<Arc<Process>>,
    files: Option<Arc<FileArray>>,
    fs_context: Option<Arc<FsContext>>,
    signal_list: Option<SignalList>,
    tls: Option<usize>,
    set_child_tid: Option<usize>,
    clear_child_tid: Option<usize>,

    trap_ctx: Option<TrapContext>,
    fpu_state: Option<FpuState>,
}

#[derive(Debug)]
struct ThreadInner {
    /// Thread name
    name: Arc<[u8]>,

    /// Thread TLS
    tls: Option<UserTLS>,

    /// User pointer
    /// Store child thread's tid when child thread returns to user space.
    set_child_tid: Option<usize>,

    clear_child_tid: Option<usize>,
}

pub struct Thread {
    pub tid: u32,
    pub process: Arc<Process>,

    pub files: Arc<FileArray>,
    pub fs_context: Arc<FsContext>,

    pub signal_list: SignalList,

    pub trap_ctx: AtomicUniqueRefCell<TrapContext>,
    pub fpu_state: AtomicUniqueRefCell<FpuState>,

    pub dead: AtomicBool,

    inner: Spin<ThreadInner>,
}

impl ThreadBuilder {
    pub fn new() -> Self {
        Self {
            tid: None,
            name: None,
            process: None,
            files: None,
            fs_context: None,
            signal_list: None,
            tls: None,
            set_child_tid: None,
            clear_child_tid: None,
            trap_ctx: None,
            fpu_state: None,
        }
    }

    pub fn tid(mut self, tid: u32) -> Self {
        self.tid = Some(tid);
        self
    }

    pub fn name(mut self, name: Arc<[u8]>) -> Self {
        self.name = Some(name);
        self
    }

    pub fn process(mut self, process: Arc<Process>) -> Self {
        self.process = Some(process);
        self
    }

    pub fn files(mut self, files: Arc<FileArray>) -> Self {
        self.files = Some(files);
        self
    }

    pub fn fs_context(mut self, fs_context: Arc<FsContext>) -> Self {
        self.fs_context = Some(fs_context);
        self
    }

    pub fn signal_list(mut self, signal_list: SignalList) -> Self {
        self.signal_list = Some(signal_list);
        self
    }

    pub fn tls(mut self, tls: Option<usize>) -> Self {
        self.tls = tls;
        self
    }

    pub fn set_child_tid(mut self, set_child_tid: Option<usize>) -> Self {
        self.set_child_tid = set_child_tid;
        self
    }

    pub fn clear_child_tid(mut self, clear_child_tid: Option<usize>) -> Self {
        self.clear_child_tid = clear_child_tid;
        self
    }

    pub fn trap_ctx(mut self, trap_ctx: TrapContext) -> Self {
        self.trap_ctx = Some(trap_ctx);
        self
    }

    pub fn fpu_state(mut self, fpu_state: FpuState) -> Self {
        self.fpu_state = Some(fpu_state);
        self
    }

    pub fn entry(mut self, entry: VAddr, stack_pointer: VAddr) -> Self {
        let mut trap_ctx = TrapContext::new();
        trap_ctx.set_user_mode(true);
        trap_ctx.set_program_counter(entry.addr());
        trap_ctx.set_stack_pointer(stack_pointer.addr());
        trap_ctx.set_interrupt_enabled(true);

        self.trap_ctx = Some(trap_ctx);
        self
    }

    /// Clone the thread from another thread.
    pub fn clone_from(self, thread: &Thread, clone_args: &CloneArgs) -> KResult<Self> {
        let inner = thread.inner.lock();

        let mut trap_ctx = thread.trap_ctx.borrow().clone();
        trap_ctx.set_user_return_value(0);

        if let Some(sp) = clone_args.sp {
            trap_ctx.set_stack_pointer(sp);
        }

        let tls = if clone_args.flags.contains(CloneFlags::CLONE_SETTLS) {
            Some(clone_args.tls)
        } else {
            None
        };

        let fs_context = if clone_args.flags.contains(CloneFlags::CLONE_FS) {
            FsContext::new_shared(&thread.fs_context)
        } else {
            FsContext::new_cloned(&thread.fs_context)
        };

        let files = if clone_args.flags.contains(CloneFlags::CLONE_FILES) {
            FileArray::new_shared(&thread.files)
        } else {
            FileArray::new_cloned(&thread.files)
        };

        let signal_list = if clone_args.flags.contains(CloneFlags::CLONE_SIGHAND) {
            SignalList::new_shared(&thread.signal_list)
        } else {
            SignalList::new_cloned(&thread.signal_list)
        };

        let set_child_tid = if clone_args.flags.contains(CloneFlags::CLONE_CHILD_SETTID) {
            Some(clone_args.child_tid_ptr)
        } else {
            None
        };

        let clear_child_tid = if clone_args.flags.contains(CloneFlags::CLONE_CHILD_CLEARTID) {
            Some(clone_args.child_tid_ptr)
        } else {
            None
        };

        Ok(self
            .files(files)
            .fs_context(fs_context)
            .signal_list(signal_list)
            .name(inner.name.clone())
            .tls(tls)
            .set_child_tid(set_child_tid)
            .clear_child_tid(clear_child_tid)
            .trap_ctx(trap_ctx)
            .fpu_state(thread.fpu_state.borrow().clone()))
    }

    pub fn build(self, process_list: &mut ProcessList) -> Arc<Thread> {
        let tid = self.tid.expect("TID is not set");
        let name = self.name.expect("Name is not set");
        let process = self.process.expect("Process is not set");
        let files = self.files.unwrap_or_else(|| FileArray::new());
        let fs_context = self
            .fs_context
            .unwrap_or_else(|| FsContext::global().clone());
        let signal_list = self.signal_list.unwrap_or_else(|| SignalList::new());
        let trap_ctx = self.trap_ctx.expect("TrapContext is not set");
        let fpu_state = self.fpu_state.unwrap_or_else(FpuState::new);

        signal_list.clear_pending();

        let thread = Arc::new(Thread {
            tid,
            process: process.clone(),
            files,
            fs_context,
            signal_list,
            trap_ctx: AtomicUniqueRefCell::new(trap_ctx),
            fpu_state: AtomicUniqueRefCell::new(fpu_state),
            dead: AtomicBool::new(false),
            inner: Spin::new(ThreadInner {
                name,
                tls: None,
                set_child_tid: self.set_child_tid,
                clear_child_tid: self.clear_child_tid,
            }),
        });

        process_list.add_thread(&thread);
        process.add_thread(&thread, process_list.prove_mut());
        thread
    }
}

impl Thread {
    pub fn current<'lt>() -> BorrowedArc<'lt, Self> {
        // SAFETY: We won't change the thread pointer in the current CPU when
        // we return here after some preemption.
        let current = CURRENT_THREAD.get().expect("Current thread is not set");

        // SAFETY: We can only use the returned value when we are in the context of the thread.
        unsafe { BorrowedArc::from_raw(current) }
    }

    pub fn raise(&self, signal: Signal) -> RaiseResult {
        self.signal_list.raise(signal)
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_thread_area32(&self) {
        if let Some(tls) = self.inner.lock().tls.as_ref() {
            CPU::local().as_mut().set_tls32(tls);
        }
    }

    pub fn set_user_tls(&self, arch_tls: usize) -> KResult<()> {
        #[cfg(target_arch = "x86_64")]
        {
            let desc = arch_tls as *mut UserDescriptor;

            let desc_pointer = UserPointerMut::new(desc)?;
            let mut desc = desc_pointer.read()?;

            // Clear the TLS area if it is not present.
            if desc.flags.is_read_exec_only() && !desc.flags.is_present() {
                if desc.limit == 0 || desc.base == 0 {
                    return Ok(());
                }

                let len = if desc.flags.is_limit_in_pages() {
                    (desc.limit as usize) << 12
                } else {
                    desc.limit as usize
                };

                CheckedUserPointer::new(desc.base as _, len)?.zero()?;
                return Ok(());
            }

            let (tls, entry) =
                UserTLS::new32(desc.base, desc.limit, desc.flags.is_limit_in_pages());
            desc.entry = entry;

            self.inner.lock().tls = Some(tls);
            desc_pointer.write(desc)?;
        }

        Ok(())
    }

    pub fn set_name(&self, name: Arc<[u8]>) {
        self.inner.lock().name = name;
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        self.inner.lock().name.clone()
    }

    pub fn set_child_tid(&self, set_child_tid: Option<usize>) {
        self.inner.lock().set_child_tid = set_child_tid;
    }

    pub fn clear_child_tid(&self, clear_child_tid: Option<usize>) {
        self.inner.lock().clear_child_tid = clear_child_tid;
    }

    pub fn get_set_ctid(&self) -> Option<usize> {
        self.inner.lock().set_child_tid
    }

    pub fn get_clear_ctid(&self) -> Option<usize> {
        self.inner.lock().clear_child_tid
    }

    pub fn handle_syscall(&self, no: usize, args: [usize; 6]) -> Option<usize> {
        match syscall_handlers().get(no) {
            Some(Some(SyscallHandler {
                handler,
                name: _name,
                ..
            })) => {
                println_trace!(
                    "trace_syscall",
                    "Syscall {_name}({no:#x}) on tid {:#x}",
                    self.tid
                );
                handler(self, args)
            }
            _ => {
                println_warn!("Syscall {no}({no:#x}) isn't implemented.");
                self.raise(Signal::SIGSYS);
                None
            }
        }
    }

    pub async fn force_kill(&self, signal: Signal) {
        let mut proc_list = ProcessList::get().write().await;
        unsafe {
            // SAFETY: Preemption is disabled.
            proc_list
                .do_exit(self, WaitType::Signaled(signal), false)
                .await;
        }
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    async fn real_run(&self) {
        if let Some(set_ctid) = self.get_set_ctid() {
            UserPointerMut::new(set_ctid as *mut u32)
                .expect("set_child_tid pointer is invalid")
                .write(self.tid)
                .expect("set_child_tid write failed");
        }

        while !self.is_dead() {
            if self.signal_list.has_pending_signal() {
                self.signal_list
                    .handle(&mut self.trap_ctx.borrow(), &mut self.fpu_state.borrow())
                    .await;
            }

            if self.is_dead() {
                return;
            }

            self.fpu_state.borrow().restore();

            unsafe {
                // SAFETY: We are returning to the context of the user thread.
                self.trap_ctx.borrow().trap_return();
            }

            self.fpu_state.borrow().save();

            let trap_type = self.trap_ctx.borrow().trap_type();
            match trap_type {
                TrapType::Fault(Fault::PageFault(err_code)) => {
                    let addr = arch::get_page_fault_address();
                    let mms = &self.process.mm_list;
                    if let Err(signal) = mms.handle_user_page_fault(addr, err_code).await {
                        self.signal_list.raise(signal);
                    }
                }
                TrapType::Fault(Fault::BadAccess) => {
                    self.signal_list.raise(Signal::SIGSEGV);
                }
                TrapType::Fault(Fault::InvalidOp) => {
                    self.signal_list.raise(Signal::SIGILL);
                }
                TrapType::Fault(Fault::Unknown(_)) => unimplemented!("Unhandled fault"),
                TrapType::Irq(irqno) => default_irq_handler(irqno),
                TrapType::Timer => {
                    timer_interrupt();
                    yield_now().await;
                }
                TrapType::Syscall { no, args } => {
                    if let Some(retval) = self.handle_syscall(no, args) {
                        self.trap_ctx.borrow().set_user_return_value(retval);
                    }
                }
            }
        }
    }

    pub async fn run(self: Arc<Thread>) {
        struct ContextedRun<'a, F: Future>(F, &'a Thread);

        impl<F: Future> Future for ContextedRun<'_, F> {
            type Output = F::Output;

            fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
                let irq_state = disable_irqs_save();
                let (future, _) = unsafe {
                    // SAFETY: We construct a pinned future and `&Thread` is `Unpin`.
                    let me = self.as_mut().get_unchecked_mut();
                    (Pin::new_unchecked(&mut me.0), me.1)
                };

                let retval = future.poll(ctx);

                irq_state.restore();
                retval
            }
        }

        ContextedRun(self.real_run(), &self).await
    }
}

async fn yield_now() {
    struct Yield {
        yielded: bool,
    }

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.as_mut().yielded {
                Poll::Ready(())
            } else {
                self.as_mut().yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    Yield { yielded: false }.await;
}

pub fn new_thread_runnable(
    thread: Arc<Thread>,
) -> ThreadRunnable<impl Future<Output = impl Send + 'static> + Send + 'static> {
    ThreadRunnable {
        thread: thread.clone(),
        future: thread.run(),
    }
}

impl<F: Future> Contexted for ThreadRunnable<F> {
    fn load_running_context(&self) {
        self.thread.process.mm_list.activate();

        let raw_ptr: *const Thread = &raw const *self.thread;
        CURRENT_THREAD.set(NonNull::new(raw_ptr as *mut _));

        unsafe {
            // SAFETY: Preemption is disabled.
            self.thread.load_thread_area32();
        }

        unsafe {
            let trap_ctx_ptr: *const TrapContext = &raw const *self.thread.trap_ctx.borrow();
            // SAFETY:
            CPU::local()
                .as_mut()
                .load_interrupt_stack(trap_ctx_ptr.add(1).addr() as u64);
        }
    }

    fn restore_running_context(&self) {
        self.thread.process.mm_list.deactivate();

        CURRENT_THREAD.set(None);
    }
}

impl<F: Future> Run for ThreadRunnable<F> {
    type Output = F::Output;

    fn run(mut self: Pin<&mut Self>, waker: &Waker) -> RunState<Self::Output> {
        let mut ctx = Context::from_waker(waker);

        match unsafe {
            self.as_mut()
                .map_unchecked_mut(|me| &mut me.future)
                .poll(&mut ctx)
        } {
            Poll::Ready(output) => RunState::Finished(output),
            Poll::Pending => RunState::Running,
        }
    }
}
