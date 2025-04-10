use crate::task::Task;
use alloc::{collections::VecDeque, sync::Arc};
use eonix_sync::Spin;

#[arch::define_percpu_shared]
static READYQUEUE: FifoReadyQueue = FifoReadyQueue::new();

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

pub fn local_rq() -> &'static Spin<dyn ReadyQueue> {
    &*READYQUEUE
}
