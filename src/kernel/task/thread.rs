use super::{
    signal::{RaiseResult, Signal, SignalList},
    Process, ProcessList,
};
use crate::{
    kernel::{
        cpu::current_cpu,
        mem::VAddr,
        user::dataflow::CheckedUserPointer,
        vfs::{filearray::FileArray, FsContext},
    },
    prelude::*,
};
use alloc::sync::Arc;
use arch::{InterruptContext, UserTLS, _arch_fork_return};
use bindings::KERNEL_PML4;
use core::{
    arch::asm,
    pin::Pin,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
    task::Waker,
};
use eonix_runtime::{
    context::ExecutionContext,
    run::{Contexted, PinRun, RunState},
};
use eonix_sync::AsProofMut as _;
use pointers::BorrowedArc;

struct CurrentThread {
    thread: NonNull<Thread>,
    runnable: NonNull<ThreadRunnable>,
}

#[arch::define_percpu]
static CURRENT_THREAD: Option<CurrentThread> = None;

pub struct ThreadBuilder {
    tid: Option<u32>,
    name: Option<Arc<[u8]>>,
    process: Option<Arc<Process>>,
    files: Option<Arc<FileArray>>,
    fs_context: Option<Arc<FsContext>>,
    signal_list: Option<SignalList>,
    tls: Option<UserTLS>,
    set_child_tid: Option<usize>,
}

#[derive(Debug)]
struct ThreadInner {
    /// Thread name
    name: Arc<[u8]>,

    /// Thread TLS
    tls: Option<UserTLS>,

    /// User pointer
    /// Store child thread's tid when child thread returns to user space.
    set_child_tid: usize,
}

pub struct Thread {
    pub tid: u32,
    pub process: Arc<Process>,

    pub files: Arc<FileArray>,
    pub fs_context: Arc<FsContext>,

    pub signal_list: SignalList,
    inner: Spin<ThreadInner>,
}

pub struct ThreadRunnable {
    thread: Arc<Thread>,
    /// Interrupt context for the thread initialization.
    /// We store the kernel stack pointer in one of the fields for now.
    ///
    /// TODO: A better way to store the interrupt context.
    interrupt_context: InterruptContext,
    interrupt_stack_pointer: AtomicUsize,
    return_context: ExecutionContext,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptorFlags(u32);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UserDescriptor {
    entry: u32,
    base: u32,
    limit: u32,
    flags: UserDescriptorFlags,
}

#[allow(dead_code)]
impl UserDescriptorFlags {
    fn is_32bit_segment(&self) -> bool {
        self.0 & 0b1 != 0
    }

    fn contents(&self) -> u32 {
        self.0 & 0b110
    }

    fn is_read_exec_only(&self) -> bool {
        self.0 & 0b1000 != 0
    }

    fn is_limit_in_pages(&self) -> bool {
        self.0 & 0b10000 != 0
    }

    fn is_present(&self) -> bool {
        self.0 & 0b100000 == 0
    }

    fn is_usable(&self) -> bool {
        self.0 & 0b1000000 != 0
    }
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

    pub fn tls(mut self, tls: Option<UserTLS>) -> Self {
        self.tls = tls;
        self
    }

    pub fn set_child_tid(mut self, set_child_tid: usize) -> Self {
        self.set_child_tid = Some(set_child_tid);
        self
    }

    /// Fork the thread from another thread.
    ///
    /// Sets the thread's files, fs_context, signal_list, name, tls, and set_child_tid
    pub fn fork_from(self, thread: &Thread) -> Self {
        let inner = thread.inner.lock();

        self.files(FileArray::new_cloned(&thread.files))
            .fs_context(FsContext::new_cloned(&thread.fs_context))
            .signal_list(thread.signal_list.clone())
            .name(inner.name.clone())
            .tls(inner.tls.clone())
            .set_child_tid(inner.set_child_tid)
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
        let set_child_tid = self.set_child_tid.unwrap_or(0);

        signal_list.clear_pending();

        let thread = Arc::new(Thread {
            tid,
            process: process.clone(),
            files,
            fs_context,
            signal_list,
            inner: Spin::new(ThreadInner {
                name,
                tls: self.tls,
                set_child_tid,
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
        let current: &Option<CurrentThread> = unsafe { CURRENT_THREAD.as_ref() };
        let current = current.as_ref().expect("Current thread is not set");

        // SAFETY: We can only use the returned value when we are in the context of the thread.
        unsafe { BorrowedArc::from_raw(current.thread) }
    }

    pub fn raise(self: &Arc<Self>, signal: Signal) -> RaiseResult {
        self.signal_list.raise(signal)
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_thread_area32(&self) {
        if let Some(tls) = self.inner.lock().tls.as_ref() {
            // SAFETY: Preemption is disabled.
            tls.load(current_cpu());
        }
    }

    pub fn set_thread_area(&self, desc: &mut UserDescriptor) -> KResult<()> {
        let mut inner = self.inner.lock();

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

        let (tls, entry) = UserTLS::new32(desc.base, desc.limit, desc.flags.is_limit_in_pages());
        desc.entry = entry;
        inner.tls = Some(tls);
        Ok(())
    }

    pub fn set_name(&self, name: Arc<[u8]>) {
        self.inner.lock().name = name;
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        self.inner.lock().name.clone()
    }

    /// # Safety
    /// This function needs to be called with preempt count == 1.
    /// We won't return so clean all the resources before calling this.
    pub unsafe fn exit() -> ! {
        // SAFETY: We won't change the thread pointer in the current CPU when
        // we return here after some preemption.
        let current: &Option<CurrentThread> = unsafe { CURRENT_THREAD.as_ref() };
        let current = current.as_ref().expect("Current thread is not set");

        // SAFETY: We can only use the `run_context` when we are in the context of the thread.
        let runnable = unsafe { current.runnable.as_ref() };

        runnable.return_context.switch_noreturn()
    }
}

impl ThreadRunnable {
    pub fn new(thread: Arc<Thread>, entry: VAddr, stack_pointer: VAddr) -> Self {
        let (VAddr(entry), VAddr(stack_pointer)) = (entry, stack_pointer);

        let mut interrupt_context = InterruptContext::default();
        interrupt_context.set_return_address(entry as _, true);
        interrupt_context.set_stack_pointer(stack_pointer as _, true);
        interrupt_context.set_interrupt_enabled(true);

        Self {
            thread,
            interrupt_context,
            interrupt_stack_pointer: AtomicUsize::new(0),
            return_context: ExecutionContext::new(),
        }
    }

    pub fn from_context(thread: Arc<Thread>, interrupt_context: InterruptContext) -> Self {
        Self {
            thread,
            interrupt_context,
            interrupt_stack_pointer: AtomicUsize::new(0),
            return_context: ExecutionContext::new(),
        }
    }
}

impl Contexted for ThreadRunnable {
    fn load_running_context(&self) {
        let thread: &Thread = &self.thread;

        match self.interrupt_stack_pointer.load(Ordering::Relaxed) {
            0 => {}
            sp => unsafe {
                // SAFETY: Preemption is disabled.
                arch::load_interrupt_stack(current_cpu(), sp as u64);
            },
        }

        // SAFETY: Preemption is disabled.
        unsafe {
            // SAFETY: `self` and `thread` are valid and non-null.
            let current_thread = CurrentThread {
                thread: NonNull::new_unchecked(thread as *const _ as *mut _),
                runnable: NonNull::new_unchecked(self as *const _ as *mut _),
            };

            // SAFETY: Preemption is disabled.
            CURRENT_THREAD.swap(Some(current_thread));
        }

        thread.process.mm_list.switch_page_table();

        unsafe {
            // SAFETY: Preemption is disabled.
            thread.load_thread_area32();
        }
    }

    fn restore_running_context(&self) {
        arch::set_root_page_table(KERNEL_PML4 as usize);
    }
}

impl PinRun for ThreadRunnable {
    type Output = ();

    fn pinned_run(self: Pin<&mut Self>, waker: &Waker) -> RunState<Self::Output> {
        let mut task_context = ExecutionContext::new();
        task_context.set_interrupt(false);
        task_context.set_ip(_arch_fork_return as _);
        task_context.set_sp(&self.interrupt_context as *const _ as _);

        self.thread.signal_list.set_signal_waker(waker.clone());

        eonix_preempt::disable();

        // TODO!!!!!: CHANGE THIS
        let sp = unsafe {
            let mut sp: usize;
            asm!(
                "mov %rsp, {0}",
                out(reg) sp,
                options(nomem, preserves_flags, att_syntax),
            );
            sp -= 512;
            sp &= !0xf;

            sp
        };

        self.interrupt_stack_pointer.store(sp, Ordering::Relaxed);

        unsafe {
            // SAFETY: Preemption is disabled.
            arch::load_interrupt_stack(current_cpu(), sp as u64);
        }

        eonix_preempt::enable();

        self.return_context.switch_to(&task_context);

        // We return here with preempt count == 1.
        eonix_preempt::enable();

        RunState::Finished(())
    }
}
