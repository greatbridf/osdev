use super::{Dentry, Inode};
use crate::{
    kernel::vfs::{s_isdir, s_islnk},
    prelude::*,
    rcu::{RCUIterator, RCUList},
};
use alloc::sync::Arc;
use bindings::ENOENT;
use core::sync::atomic::Ordering;

const DCACHE_HASH_BITS: u32 = 8;

static DCACHE: [RCUList<Dentry>; 1 << DCACHE_HASH_BITS] =
    [const { RCUList::new() }; 1 << DCACHE_HASH_BITS];

pub fn d_hinted(hash: u64) -> &'static RCUList<Dentry> {
    let hash = hash as usize & ((1 << DCACHE_HASH_BITS) - 1);
    &DCACHE[hash]
}

pub fn d_iter_for(hash: u64) -> RCUIterator<'static, Dentry> {
    d_hinted(hash).iter()
}

/// Add the dentry to the dcache
pub fn d_add(dentry: &Arc<Dentry>) {
    d_hinted(dentry.hash).insert(dentry.clone());
}

pub fn d_find_fast(dentry: &Arc<Dentry>) -> Option<Arc<Dentry>> {
    d_iter_for(dentry.rehash())
        .find(|cur| cur.hash_eq(dentry))
        .map(|dentry| dentry.clone())
}

/// Call `lookup()` on the parent inode to try find if the dentry points to a valid inode
///
/// Silently fail without any side effects
pub fn d_try_revalidate(dentry: &Arc<Dentry>) {
    (|| -> KResult<()> {
        let parent = dentry.parent().get_inode()?;
        let inode = parent.lookup(dentry)?.ok_or(ENOENT)?;

        d_save(dentry, inode)
    })()
    .unwrap_or_default();
}

/// Save the inode to the dentry.
///
/// Dentry flags will be determined by the inode's mode.
pub fn d_save(dentry: &Arc<Dentry>, inode: Arc<dyn Inode>) -> KResult<()> {
    match inode.mode.load(Ordering::Acquire) {
        mode if s_isdir(mode) => dentry.save_dir(inode),
        mode if s_islnk(mode) => dentry.save_symlink(inode),
        _ => dentry.save_reg(inode),
    }
}

/// Replace the old dentry with the new one in the dcache
pub fn d_replace(old: &Arc<Dentry>, new: Arc<Dentry>) {
    d_hinted(old.hash).replace(old, new);
}

/// Remove the dentry from the dcache so that later d_find_fast will fail
pub fn d_remove(dentry: &Arc<Dentry>) {
    d_hinted(dentry.hash).remove(&dentry);
}
