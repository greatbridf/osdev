use alloc::{collections::VecDeque, sync::Arc};

use crate::sync::Spin;

use super::Task;

#[arch::define_percpu]
static READYQUEUE: Option<Spin<FifoReadyQueue>> = None;

pub trait ReadyQueue {
    fn get(&mut self) -> Option<Arc<Task>>;
    fn put(&mut self, thread: Arc<Task>);
}

pub struct FifoReadyQueue {
    threads: VecDeque<Arc<Task>>,
}

impl FifoReadyQueue {
    pub const fn new() -> Self {
        FifoReadyQueue {
            threads: VecDeque::new(),
        }
    }
}

impl ReadyQueue for FifoReadyQueue {
    fn get(&mut self) -> Option<Arc<Task>> {
        self.threads.pop_front()
    }

    fn put(&mut self, thread: Arc<Task>) {
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

pub fn init_rq_thiscpu() {
    READYQUEUE.set(Some(Spin::new(FifoReadyQueue::new())));
}
