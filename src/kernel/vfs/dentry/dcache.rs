use super::{Dentry, Inode};
use crate::kernel::constants::ENOENT;
use crate::kernel::task::block_on;
use crate::kernel::vfs::inode::Mode;
use crate::rcu::RCUPointer;
use crate::{
    prelude::*,
    rcu::{RCUIterator, RCUList},
};
use alloc::sync::Arc;
use core::sync::atomic::Ordering;
use eonix_sync::Mutex;

const DCACHE_HASH_BITS: u32 = 8;

static DCACHE: [RCUList<Dentry>; 1 << DCACHE_HASH_BITS] =
    [const { RCUList::new() }; 1 << DCACHE_HASH_BITS];

static D_EXCHANGE_LOCK: Mutex<()> = Mutex::new(());

pub fn d_hinted(dentry: &Dentry) -> &'static RCUList<Dentry> {
    let hash = dentry.hash.load(Ordering::Relaxed) as usize & ((1 << DCACHE_HASH_BITS) - 1);
    &DCACHE[hash]
}

pub fn d_iter_for(dentry: &Dentry) -> RCUIterator<'static, Dentry> {
    d_hinted(dentry).iter()
}

/// Add the dentry to the dcache
pub fn d_add(dentry: Arc<Dentry>) {
    d_hinted(&dentry).insert(dentry);
}

pub fn d_find_fast(dentry: &Dentry) -> Option<Arc<Dentry>> {
    d_iter_for(dentry)
        .find(|cur| cur.hash_eq(dentry))
        .map(|dentry| dentry.clone())
}

/// Call `lookup()` on the parent inode to try find if the dentry points to a valid inode
///
/// Silently fail without any side effects
pub fn d_try_revalidate(dentry: &Arc<Dentry>) {
    let _lock = block_on(D_EXCHANGE_LOCK.lock());

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
    match inode.mode.load().format() {
        Mode::DIR => dentry.save_dir(inode),
        Mode::LNK => dentry.save_symlink(inode),
        _ => dentry.save_reg(inode),
    }
}

/// Replace the old dentry with the new one in the dcache
pub fn d_replace(old: &Arc<Dentry>, new: Arc<Dentry>) {
    d_hinted(old).replace(old, new);
}

/// Remove the dentry from the dcache so that later d_find_fast will fail
pub fn d_remove(dentry: &Arc<Dentry>) {
    d_hinted(dentry).remove(&dentry);
}

pub async fn d_exchange(old: &Arc<Dentry>, new: &Arc<Dentry>) {
    if Arc::ptr_eq(old, new) {
        return;
    }

    let _lock = D_EXCHANGE_LOCK.lock().await;

    d_remove(old);
    d_remove(new);

    unsafe {
        RCUPointer::exchange(&old.parent, &new.parent);
        RCUPointer::exchange(&old.name, &new.name);
    }

    old.rehash();
    new.rehash();

    d_add(old.clone());
    d_add(new.clone());
}
