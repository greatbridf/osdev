use super::Dentry;
use crate::kernel::constants::ENOENT;
use crate::rcu::{RCUPointer, RCUReadLock};
use crate::{
    prelude::*,
    rcu::{RCUIterator, RCUList},
};
use alloc::sync::Arc;
use arcref::ArcRef;
use core::ops::Deref;
use core::sync::atomic::Ordering;
use eonix_sync::Mutex;

const DCACHE_HASH_BITS: u32 = 8;

static DCACHE: [RCUList<Dentry>; 1 << DCACHE_HASH_BITS] =
    [const { RCUList::new() }; 1 << DCACHE_HASH_BITS];

static D_EXCHANGE_LOCK: Mutex<()> = Mutex::new(());

pub trait DCacheItem {
    fn d_hash(&self) -> usize;
    fn d_parent(&self) -> *const Dentry;
    fn d_name<'r, 'a: 'r, 'b: 'a>(
        &'a self,
        rcu_read: &'b RCUReadLock,
    ) -> impl Deref<Target = [u8]> + 'r;
}

fn d_eq(lhs: &impl DCacheItem, rhs: &impl DCacheItem, rcu_read: &RCUReadLock) -> bool {
    lhs.d_hash() == rhs.d_hash()
        && lhs.d_parent() == rhs.d_parent()
        && *lhs.d_name(rcu_read) == *rhs.d_name(rcu_read)
}

fn d_hinted(item: &impl DCacheItem) -> &'static RCUList<Dentry> {
    let hash = item.d_hash() & ((1 << DCACHE_HASH_BITS) - 1);
    &DCACHE[hash]
}

fn d_iter_for<'rcu>(
    item: &impl DCacheItem,
    rcu_read: &'rcu RCUReadLock,
) -> RCUIterator<'static, 'rcu, Dentry> {
    d_hinted(item).iter(rcu_read)
}

pub fn d_find_rcu<'rcu>(
    item: &impl DCacheItem,
    rcu_read: &'rcu RCUReadLock,
) -> Option<ArcRef<'rcu, Dentry>> {
    d_iter_for(item, rcu_read).find(|cur_ref| cur_ref.with_arc(|cur| d_eq(cur, item, rcu_read)))
}

/// Add the dentry to the dcache
pub fn d_add(dentry: Arc<Dentry>) {
    // TODO: Add `children` field to parent and lock parent dentry to avoid
    //       concurrent insertion causing duplication.
    d_hinted(&dentry).insert(dentry);
}

/// Call `lookup()` on the parent inode to try find if the dentry points to a valid inode
///
/// Silently fail without any side effects
pub async fn d_try_revalidate(dentry: &Arc<Dentry>) -> KResult<()> {
    let _lock = D_EXCHANGE_LOCK.lock().await;

    let parent = dentry.parent().get_inode()?;
    let inode = parent.lookup(dentry).await?.ok_or(ENOENT)?;

    dentry.fill(inode);
    Ok(())
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

impl DCacheItem for Arc<Dentry> {
    fn d_hash(&self) -> usize {
        self.hash.load(Ordering::Relaxed) as usize
    }

    fn d_parent(&self) -> *const Dentry {
        self.parent_addr()
    }

    fn d_name<'r, 'a: 'r, 'b: 'a>(
        &'a self,
        rcu_read: &'b RCUReadLock,
    ) -> impl Deref<Target = [u8]> + 'r {
        struct Name<'a>(ArcRef<'a, Arc<[u8]>>);

        impl Deref for Name<'_> {
            type Target = [u8];

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        Name(
            self.name
                .dereference(rcu_read)
                .expect("Dentry should have a non-null name"),
        )
    }
}
