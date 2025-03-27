use alloc::{collections::VecDeque, sync::Arc};

use crate::{println_debug, sync::Spin};

use super::Thread;

#[arch::define_percpu]
static READYQUEUE: Option<Spin<FifoReadyQueue>> = None;

pub trait ReadyQueue {
    fn get(&mut self) -> Option<Arc<Thread>>;
    fn put(&mut self, thread: Arc<Thread>);
}

pub struct FifoReadyQueue {
    threads: VecDeque<Arc<Thread>>,
}

impl FifoReadyQueue {
    pub const fn new() -> Self {
        FifoReadyQueue {
            threads: VecDeque::new(),
        }
    }
}

impl ReadyQueue for FifoReadyQueue {
    fn get(&mut self) -> Option<Arc<Thread>> {
        self.threads.pop_front()
    }

    fn put(&mut self, thread: Arc<Thread>) {
        self.threads.push_back(thread);
    }
}

pub fn rq_thiscpu() -> &'static Spin<dyn ReadyQueue> {
    // SAFETY: When we use ReadyQueue on this CPU, we will lock it with `lock_irq()`
    //         and if we use ReadyQueue on other CPU, we won't be able to touch it on this CPU.
    //         So no issue here.
    unsafe { READYQUEUE.as_ref() }
        .as_ref()
        .expect("ReadyQueue should be initialized")
}

pub unsafe fn init_rq_thiscpu() {
    READYQUEUE.set(Some(Spin::new(FifoReadyQueue::new())));
}
