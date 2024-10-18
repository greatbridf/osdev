use core::{mem::MaybeUninit, sync::atomic::AtomicPtr};

use alloc::sync::Arc;
use bindings::ENOENT;

use crate::{
    kernel::vfs::{s_isdir, s_islnk},
    prelude::*,
    rcu::{RCUIterator, RCUList, RCUPointer},
};

use super::{Dentry, Inode};

use lazy_static::lazy_static;

const DCACHE_HASH_BITS: u32 = 8;

static DCACHE: [RCUList<Dentry>; 1 << DCACHE_HASH_BITS] =
    [const { RCUList::new() }; 1 << DCACHE_HASH_BITS];

lazy_static! {
    static ref DROOT: Arc<Dentry> = {
        let dentry = Arc::new_uninit();
        let fake_parent = unsafe { dentry.clone().assume_init() };

        unsafe { &mut *(Arc::as_ptr(&dentry) as *mut MaybeUninit<Dentry>) }
            .write(Dentry {
                parent: fake_parent,
                name: b"[root]".as_slice().into(),
                hash: 0,
                prev: AtomicPtr::default(),
                next: AtomicPtr::default(),
                data: RCUPointer::empty(),
            });

        unsafe { dentry.assume_init() }
    };
}

pub fn _looped_droot() -> &'static Arc<Dentry> {
    &DROOT
}

pub fn d_hinted(hash: u64) -> &'static RCUList<Dentry> {
    let hash = hash as usize & ((1 << DCACHE_HASH_BITS) - 1);
    &DCACHE[hash]
}

pub fn d_iter_for(hash: u64) -> RCUIterator<'static, Dentry> {
    d_hinted(hash).iter()
}

pub fn d_add(dentry: &Arc<Dentry>) {
    d_hinted(dentry.hash).insert(dentry.clone());
}

pub fn d_find_fast(dentry: &Arc<Dentry>) -> Option<Arc<Dentry>> {
    d_iter_for(dentry.rehash())
        .find(|cur| cur.hash_eq(dentry))
        .map(|dentry| dentry.clone())
}

/// Silently fail without any side effects
pub fn d_try_revalidate(dentry: &Arc<Dentry>) {
    (|| -> KResult<()> {
        let parent = dentry.parent().get_inode()?;
        let inode = parent.lookup(&parent, dentry)?.ok_or(ENOENT)?;

        d_save(dentry, inode)
    })()
    .unwrap_or_default();
}

pub fn d_save(dentry: &Arc<Dentry>, inode: Arc<Inode>) -> KResult<()> {
    let mode = inode.idata.lock().mode;
    match mode {
        mode if s_isdir(mode) => dentry.save_dir(inode),
        mode if s_islnk(mode) => dentry.save_symlink(inode),
        _ => dentry.save_reg(inode),
    }
}

pub fn d_replace(old: &Arc<Dentry>, new: Arc<Dentry>) {
    d_hinted(old.hash).replace(old, new);
}
