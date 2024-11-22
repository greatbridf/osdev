use core::sync::atomic::{compiler_fence, Ordering};

use crate::{prelude::*, sync::preempt};

use alloc::{
    collections::vec_deque::VecDeque,
    sync::{Arc, Weak},
};
use lazy_static::lazy_static;

use super::{Thread, ThreadState};

pub struct Scheduler {
    ready: VecDeque<Weak<Thread>>,
}

/// Idle task thread
///
/// # Safety
/// This variable is per cpu. So no need to synchronize accesses to it.
///
/// TODO!!!: This should be per cpu in smp environment.
static mut IDLE_TASK: Option<Arc<Thread>> = None;

/// Current thread
///
/// # Safety
/// This variable is per cpu. So no need to synchronize accesses to it.
///
/// TODO!!!: This should be per cpu in smp environment.
static mut CURRENT: Option<Arc<Thread>> = None;

lazy_static! {
    static ref GLOBAL_SCHEDULER: Spin<Scheduler> = Spin::new(Scheduler {
        ready: VecDeque::new(),
    });
}

impl Scheduler {
    /// `Scheduler` might be used in various places. Do not hold it for a long time.
    ///
    /// # Safety
    /// The locked returned by this function should be locked with `lock_irq` to prevent from
    /// rescheduling during access to the scheduler. Disabling preemption will do the same.
    ///
    /// Drop the lock before calling `schedule`.
    pub fn get() -> &'static Spin<Self> {
        &GLOBAL_SCHEDULER
    }

    pub fn current<'lt>() -> &'lt Arc<Thread> {
        // SAFETY: `CURRENT` is per cpu.
        unsafe { CURRENT.as_ref().unwrap() }
    }

    pub fn idle_task() -> &'static Arc<Thread> {
        // SAFETY: `IDLE_TASK` is per cpu.
        unsafe { IDLE_TASK.as_ref().unwrap() }
    }

    pub(super) fn set_idle(thread: Arc<Thread>) {
        thread.prepare_kernel_stack(|kstack| {
            let mut writer = kstack.get_writer();
            writer.flags = 0x200;
            writer.entry = idle_task;
            writer.finish();
        });
        // We don't wake the idle thread to prevent from accidentally being scheduled there.

        // TODO!!!: Set per cpu variable.
        unsafe { IDLE_TASK = Some(thread) };
    }

    pub(super) fn set_current(thread: Arc<Thread>) {
        // TODO!!!: Set per cpu variable.
        unsafe { CURRENT = Some(thread) };
    }

    fn enqueue(&mut self, thread: &Arc<Thread>) {
        self.ready.push_back(Arc::downgrade(thread));
    }

    pub fn usleep(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::Running);
        // No need to dequeue. We have proved that the thread is running so not in the queue.

        *state = ThreadState::USleep;
    }

    pub fn uwake(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::USleep);

        *state = ThreadState::Ready;
        self.enqueue(&thread);
    }

    pub fn isleep(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::Running);
        // No need to dequeue. We have proved that the thread is running so not in the queue.

        *state = ThreadState::ISleep;
    }

    pub fn iwake(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();

        match *state {
            ThreadState::USleep => return,
            ThreadState::ISleep => {
                *state = ThreadState::Ready;
                self.enqueue(&thread);
            }
            state => panic!("Invalid transition from state {:?} to `Ready`", state),
        }
    }

    /// Put `Running` thread into `Ready` state and enqueue the task.
    pub fn put_ready(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::Running);

        *state = ThreadState::Ready;
        self.enqueue(&thread);
    }

    /// Set `Ready` threads to the `Running` state.
    pub fn set_running(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::Ready);

        *state = ThreadState::Running;
        // No need to dequeue. We got the thread from the queue.
    }

    /// Set `Running` threads to the `Zombie` state.
    pub fn set_zombie(&mut self, thread: &Arc<Thread>) {
        let mut state = thread.state.lock();
        assert_eq!(*state, ThreadState::Running);

        *state = ThreadState::Zombie;
    }
}

impl Scheduler {
    /// Go to idle task. Call this with `preempt_count == 1`.
    /// The preempt count will be decremented by this function.
    ///
    /// # Safety
    /// We might never return from here.
    /// Drop all variables that take ownership of some resource before calling this function.
    pub fn schedule() {
        might_sleep!(1);

        // Make sure all works are done before scheduling.
        compiler_fence(Ordering::SeqCst);

        // TODO!!!!!: Use of reference here needs further consideration.
        //
        // Since we might never return to here, we can't take ownership of `current()`.
        // Is it safe to believe that `current()` will never change across calls?
        context_switch_light(Thread::current(), Scheduler::idle_task());
        preempt::enable();
    }

    pub fn schedule_noreturn() -> ! {
        preempt::disable();
        Self::schedule();
        panic!("Scheduler::schedule_noreturn(): Should never return")
    }
}

fn context_switch_light(from: &Arc<Thread>, to: &Arc<Thread>) {
    unsafe {
        arch::task::context_switch_light(from.get_sp_ptr(), to.get_sp_ptr());
    }
}

/// In this function, we should see `preempt_count == 1`.
extern "C" fn idle_task() {
    loop {
        debug_assert_eq!(preempt::count(), 1);

        // SAFETY: No need to call `lock_irq`, preempt is already disabled.
        let mut scheduler = Scheduler::get().lock();
        let state = *Thread::current().state.lock();

        // Previous thread is `Running`.
        if let ThreadState::Running = state {
            // No other thread to run, return to current running thread without changing its state.
            if scheduler.ready.is_empty() {
                drop(scheduler);
                context_switch_light(Scheduler::idle_task(), Thread::current());
                continue;
            } else {
                // Put it into `Ready` state
                scheduler.put_ready(Thread::current());
            }
        }

        // No thread to run, halt the cpu and rerun the loop.
        if scheduler.ready.is_empty() {
            drop(scheduler);
            arch::task::halt();
            continue;
        }

        let next_thread = scheduler
            .ready
            .pop_front()
            .as_ref()
            .map(|weak| weak.upgrade().unwrap())
            .expect("We should have a thread to run");
        scheduler.set_running(&next_thread);
        drop(scheduler);

        next_thread.process.mm_list.switch_page_table();
        unsafe { CURRENT = Some(next_thread) };

        Thread::current().load_interrupt_stack();
        Thread::current().load_thread_area32();

        // TODO!!!: If the task comes from another cpu, we need to sync.
        //
        // The other cpu should see the changes of kernel stack of the target thread
        // made in this cpu.
        context_switch_light(Scheduler::idle_task(), Thread::current());
    }
}
