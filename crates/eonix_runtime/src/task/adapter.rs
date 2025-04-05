use alloc::sync::Arc;
use intrusive_collections::{intrusive_adapter, KeyAdapter, RBTreeAtomicLink};

use super::{Task, TaskId};

intrusive_adapter!(pub TaskAdapter = Arc<Task>: Task { link_task_list: RBTreeAtomicLink });

impl<'a> KeyAdapter<'a> for TaskAdapter {
    type Key = TaskId;
    fn get_key(&self, task: &'a Task) -> Self::Key {
        task.id
    }
}
