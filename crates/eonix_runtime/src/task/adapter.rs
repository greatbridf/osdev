use super::{Task, TaskId};
use alloc::sync::Arc;
use intrusive_collections::{intrusive_adapter, KeyAdapter, LinkedListAtomicLink, RBTreeAtomicLink};

intrusive_adapter!(pub TaskAdapter = Arc<Task>: Task { link_task_list: RBTreeAtomicLink });
intrusive_adapter!(pub TaskRqAdapter = Arc<Task>: Task { link_ready_queue: LinkedListAtomicLink });

impl<'a> KeyAdapter<'a> for TaskAdapter {
    type Key = TaskId;
    fn get_key(&self, task: &'a Task) -> Self::Key {
        task.id
    }
}
