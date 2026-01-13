use alloc::sync::Arc;

use super::inode::InodeUse;
use crate::kernel::vfs::dentry::Dentry;

pub enum WriteOffset<'end> {
    Position(usize),
    End(&'end mut usize),
}

pub struct RenameData<'a, 'b> {
    pub old_dentry: &'a Arc<Dentry>,
    pub new_dentry: &'b Arc<Dentry>,
    pub new_parent: InodeUse,
    pub is_exchange: bool,
    pub no_replace: bool,
}
