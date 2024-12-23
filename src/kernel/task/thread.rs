use core::{
    arch::naked_asm,
    cell::{RefCell, UnsafeCell},
    sync::atomic::AtomicBool,
};

use crate::{
    kernel::{cpu::current_cpu, user::dataflow::CheckedUserPointer, vfs::FsContext},
    prelude::*,
    sync::{preempt, AsRefMutPosition as _, AsRefPosition as _},
};

use alloc::sync::Arc;

use crate::kernel::vfs::filearray::FileArray;

use super::{
    signal::{RaiseResult, Signal, SignalList},
    KernelStack, Process, ProcessList, Scheduler, WaitObject, WaitType,
};

use arch::{InterruptContext, TaskContext, UserTLS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Preparing,
    Running,
    Ready,
    Zombie,
    ISleep,
    USleep,
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

    /// Thread state for scheduler use.
    pub state: Spin<ThreadState>,

    pub oncpu: AtomicBool,

    /// Thread context
    pub context: UnsafeCell<TaskContext>,

    /// Kernel stack
    /// Never access this directly.
    ///
    /// We can only touch kernel stack when the process is neither running nor sleeping.
    /// AKA, the process is in the ready queue and will return to `schedule` context.
    kstack: RefCell<KernelStack>,

    inner: Spin<ThreadInner>,
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

impl Thread {
    pub unsafe fn new_for_init(
        name: Arc<[u8]>,
        tid: u32,
        process: &Arc<Process>,
        procs: &mut ProcessList,
    ) -> Arc<Self> {
        let thread = Arc::new(Self {
            tid,
            process: process.clone(),
            files: FileArray::new_for_init(),
            fs_context: FsContext::new_for_init(),
            signal_list: SignalList::new(),
            kstack: RefCell::new(KernelStack::new()),
            context: UnsafeCell::new(TaskContext::new()),
            state: Spin::new(ThreadState::Preparing),
            oncpu: AtomicBool::new(false),
            inner: Spin::new(ThreadInner {
                name,
                tls: None,
                set_child_tid: 0,
            }),
        });

        process.add_thread(&thread, procs.as_pos_mut());
        thread
    }

    pub fn new_cloned(&self, procs: &mut ProcessList) -> Arc<Self> {
        let process = Process::new_cloned(&self.process, procs);

        let state = self.state.lock();
        let inner = self.inner.lock();
        assert!(matches!(*state, ThreadState::Running));

        let signal_list = self.signal_list.clone();
        signal_list.clear_pending();

        let thread = Arc::new(Self {
            tid: process.pid,
            process: process.clone(),
            files: FileArray::new_cloned(&self.files),
            fs_context: FsContext::new_cloned(&self.fs_context),
            signal_list,
            kstack: RefCell::new(KernelStack::new()),
            context: UnsafeCell::new(TaskContext::new()),
            state: Spin::new(ThreadState::Preparing),
            oncpu: AtomicBool::new(false),
            inner: Spin::new(ThreadInner {
                name: inner.name.clone(),
                tls: inner.tls.clone(),
                set_child_tid: inner.set_child_tid,
            }),
        });

        procs.add_thread(&thread);
        process.add_thread(&thread, procs.as_pos_mut());
        thread
    }

    pub fn current<'lt>() -> BorrowedArc<'lt, Self> {
        Scheduler::current()
    }

    pub fn do_stop(self: &Arc<Self>, signal: Signal) {
        if let Some(parent) = self.process.parent.load() {
            parent.notify(
                WaitObject {
                    pid: self.process.pid,
                    code: WaitType::Stopped(signal),
                },
                ProcessList::get().lock_shared().as_pos(),
            );
        }

        preempt::disable();

        // `SIGSTOP` can only be waken up by `SIGCONT` or `SIGKILL`.
        // SAFETY: Preempt disabled above.
        Scheduler::get().lock().usleep(self);
        Scheduler::schedule();
    }

    pub fn do_continue(self: &Arc<Self>) {
        if let Some(parent) = self.process.parent.load() {
            parent.notify(
                WaitObject {
                    pid: self.process.pid,
                    code: WaitType::Continued,
                },
                ProcessList::get().lock_shared().as_pos(),
            );
        }
    }

    pub fn raise(self: &Arc<Thread>, signal: Signal) -> RaiseResult {
        match self.signal_list.raise(signal) {
            RaiseResult::ShouldIWakeUp => {
                Scheduler::get().lock_irq().iwake(self);
                RaiseResult::Finished
            }
            RaiseResult::ShouldUWakeUp => {
                Scheduler::get().lock_irq().uwake(self);
                RaiseResult::Finished
            }
            result => result,
        }
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

    pub fn fork_init(&self, interrupt_context: InterruptContext) {
        let mut state = self.state.lock();
        *state = ThreadState::USleep;

        let sp = self.kstack.borrow().init(interrupt_context);
        unsafe {
            self.get_context_mut_ptr()
                .as_mut()
                .unwrap()
                .init(fork_return as usize, sp);
        }
    }

    pub fn init(&self, entry: usize) {
        let mut state = self.state.lock();
        *state = ThreadState::USleep;
        unsafe {
            self.get_context_mut_ptr()
                .as_mut()
                .unwrap()
                .init(entry, self.get_kstack_bottom());
        }
    }

    /// # Safety
    /// This function is unsafe because it accesses the `current_cpu()`, which needs
    /// to be called in a preemption disabled context.
    pub unsafe fn load_interrupt_stack(&self) {
        self.kstack.borrow().load_interrupt_stack();
    }

    pub fn get_kstack_bottom(&self) -> usize {
        self.kstack.borrow().get_stack_bottom()
    }

    pub unsafe fn get_context_mut_ptr(&self) -> *mut TaskContext {
        self.context.get()
    }

    pub fn set_name(&self, name: Arc<[u8]>) {
        self.inner.lock().name = name;
    }

    pub fn get_name(&self) -> Arc<[u8]> {
        self.inner.lock().name.clone()
    }
}

#[naked]
unsafe extern "C" fn fork_return() {
    // We don't land on the typical `Scheduler::schedule()` function, so we need to
    // manually enable preemption.
    naked_asm! {
        "
        call {preempt_enable}
        swapgs
        pop %rax
        pop %rbx
        pop %rcx
        pop %rdx
        pop %rdi
        pop %rsi
        pop %r8
        pop %r9
        pop %r10
        pop %r11
        pop %r12
        pop %r13
        pop %r14
        pop %r15
        pop %rbp
        add $16, %rsp
        iretq
        ",
        preempt_enable = sym preempt::enable,
        options(att_syntax),
    }
}

// TODO: Maybe we can find a better way instead of using `RefCell` for `KernelStack`?
unsafe impl Sync for Thread {}
