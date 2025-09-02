use alloc::sync::Arc;

use crate::kernel::vfs::dentry::Dentry;

use super::{inode::InodeUse, Inode};

pub enum WriteOffset<'end> {
    Position(usize),
    End(&'end mut usize),
}

pub struct RenameData<'a, 'b> {
    pub old_dentry: &'a Arc<Dentry>,
    pub new_dentry: &'b Arc<Dentry>,
    pub new_parent: InodeUse<dyn Inode>,
    pub is_exchange: bool,
    pub no_replace: bool,
}
