use core::{
    ptr::NonNull,
    sync::atomic::{compiler_fence, fence, Ordering},
};

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
/// All the idle task threads belongs to `pid 0` and are pinned to the current cpu.
#[arch::define_percpu]
static IDLE_TASK: Option<NonNull<Thread>> = None;

/// Current thread
#[arch::define_percpu]
static mut CURRENT: Option<NonNull<Thread>> = None;

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

    /// # Safety
    /// We should never "inspect" a change in `current`.
    /// The change of `CURRENT` will only happen in the scheduler. And if we are preempted,
    /// when we DO return, the `CURRENT` will be the same and remain valid.
    pub fn current<'lt>() -> BorrowedArc<'lt, Thread> {
        BorrowedArc::from_raw(CURRENT.get().unwrap().as_ptr())
    }

    /// # Safety
    /// Idle task should never change so we can borrow it without touching the refcount.
    pub fn idle_task() -> BorrowedArc<'static, Thread> {
        BorrowedArc::from_raw(IDLE_TASK.get().unwrap().as_ptr())
    }

    pub unsafe fn set_idle(thread: Arc<Thread>) {
        // We don't wake the idle thread to prevent from accidentally being scheduled there.
        thread.init(idle_task as *const () as usize);

        let old = IDLE_TASK.swap(NonNull::new(Arc::into_raw(thread) as *mut _));
        assert!(old.is_none(), "Idle task is already set");
    }

    pub unsafe fn set_current(thread: Arc<Thread>) {
        assert_eq!(thread.oncpu.swap(true, Ordering::AcqRel), false);
        let old = CURRENT.swap(NonNull::new(Arc::into_raw(thread) as *mut _));

        if let Some(thread_pointer) = old {
            let thread = Arc::from_raw(thread_pointer.as_ptr());
            thread.oncpu.store(false, Ordering::Release);
        }
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

        if thread.oncpu.load(Ordering::Acquire) {
            *state = ThreadState::Running;
        } else {
            *state = ThreadState::Ready;
            self.enqueue(&thread);
        }
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
            ThreadState::Ready | ThreadState::Running | ThreadState::USleep => return,
            ThreadState::ISleep => {
                if thread.oncpu.load(Ordering::Acquire) {
                    *state = ThreadState::Running;
                } else {
                    *state = ThreadState::Ready;
                    self.enqueue(&thread);
                }
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
        context_switch_light(&Thread::current(), &Scheduler::idle_task());
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
        arch::TaskContext::switch_to(
            &mut *from.get_context_mut_ptr(),
            &mut *to.get_context_mut_ptr(),
        );
    }
}

/// In this function, we should see `preempt_count == 1`.
extern "C" fn idle_task() {
    loop {
        debug_assert_eq!(preempt::count(), 1);

        let mut scheduler = Scheduler::get().lock_irq();
        let state = *Thread::current().state.lock();

        // Previous thread is `Running`.
        if let ThreadState::Running = state {
            // No other thread to run, return to current running thread without changing its state.
            if scheduler.ready.is_empty() {
                drop(scheduler);
                context_switch_light(&Scheduler::idle_task(), &Thread::current());
                continue;
            } else {
                // Put it into `Ready` state
                scheduler.put_ready(&Thread::current());
            }
        }

        // No thread to run, halt the cpu and rerun the loop.
        if scheduler.ready.is_empty() {
            drop(scheduler);
            arch::halt();
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
        unsafe {
            Scheduler::set_current(next_thread);
        }

        unsafe {
            // SAFETY: We are in the idle task where preemption is disabled.
            //         So we can safely load the thread area and interrupt stack.
            Thread::current().load_interrupt_stack();
            Thread::current().load_thread_area32();
        }

        // TODO!!!: If the task comes from another cpu, we need to sync.
        //
        // The other cpu should see the changes of kernel stack of the target thread
        // made in this cpu.
        //
        // Can we find a better way other than `fence`s?
        fence(Ordering::SeqCst);
        context_switch_light(&Scheduler::idle_task(), &Thread::current());
        fence(Ordering::SeqCst);
    }
}
