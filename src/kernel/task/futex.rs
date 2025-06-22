use core::pin::pin;

use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::bitflags;
use eonix_sync::{LazyLock, Mutex, MutexGuard, WaitList};
use intrusive_collections::{intrusive_adapter, KeyAdapter, RBTree, RBTreeAtomicLink};

use crate::{
    kernel::{
        constants::{EAGAIN, EINVAL},
        user::UserPointer,
    },
    prelude::KResult,
};

#[derive(PartialEq, Debug, Clone, Copy)]
#[repr(u32)]
#[expect(non_camel_case_types)]
pub enum FutexOp {
    FUTEX_WAIT = 0,
    FUTEX_WAKE = 1,
    FUTEX_FD = 2,
    FUTEX_REQUEUE = 3,
    FUTEX_CMP_REQUEUE = 4,
    FUTEX_WAKE_OP = 5,
    FUTEX_LOCK_PI = 6,
    FUTEX_UNLOCK_PI = 7,
    FUTEX_TRYLOCK_PI = 8,
    FUTEX_WAIT_BITSET = 9,
    FUTEX_WAKE_BITSET = 10,
}

impl TryFrom<u32> for FutexOp {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FutexOp::FUTEX_WAIT),
            1 => Ok(FutexOp::FUTEX_WAKE),
            2 => Ok(FutexOp::FUTEX_FD),
            3 => Ok(FutexOp::FUTEX_REQUEUE),
            4 => Ok(FutexOp::FUTEX_CMP_REQUEUE),
            5 => Ok(FutexOp::FUTEX_WAKE_OP),
            6 => Ok(FutexOp::FUTEX_LOCK_PI),
            7 => Ok(FutexOp::FUTEX_UNLOCK_PI),
            8 => Ok(FutexOp::FUTEX_TRYLOCK_PI),
            9 => Ok(FutexOp::FUTEX_WAIT_BITSET),
            10 => Ok(FutexOp::FUTEX_WAKE_BITSET),
            _ => Err(EINVAL),
        }
    }
}

bitflags! {
    pub struct FutexFlags : u32 {
        const FUTEX_PRIVATE         = 128;
        const FUTEX_CLOCK_REALTIME  = 256;
    }
}

const FUTEX_OP_MASK: u32 = 0x0000_000F;
const FUTEX_FLAGS_MASK: u32 = 0xFFFF_FFF0;

pub fn parse_futexop(bits: u32) -> KResult<(FutexOp, FutexFlags)> {
    let op = FutexOp::try_from(bits & FUTEX_OP_MASK)?;

    let flags = {
        let flags_bits = bits & FUTEX_FLAGS_MASK;
        FutexFlags::from_bits(flags_bits).ok_or(EINVAL)
    }?;

    Ok((op, flags))
}

struct FutexTable {
    futex_buckets: Vec<Mutex<FutexBucket>>,
}

struct FutexBucket {
    futex_items: RBTree<FutexItemAdapter>,
}

intrusive_adapter!(
    FutexItemAdapter = Arc<FutexItem>: FutexItem { link: RBTreeAtomicLink }
);

impl<'a> KeyAdapter<'a> for FutexItemAdapter {
    type Key = FutexKey;
    fn get_key(&self, item: &'a FutexItem) -> Self::Key {
        item.key
    }
}

struct FutexItem {
    link: RBTreeAtomicLink,
    key: FutexKey,
    // // A list of waiters that are waiting on this futex.
    wait_list: WaitList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FutexKey {
    addr: usize,
    pid: Option<u32>,
}

impl FutexKey {
    fn new(addr: usize, pid: Option<u32>) -> Self {
        FutexKey { addr, pid }
    }

    fn hash(&self) -> usize {
        (self.addr >> 2) + self.pid.unwrap_or(0) as usize
    }
}

static FUTEX_TABLE: LazyLock<FutexTable> = LazyLock::new(FutexTable::new);

const FUTEX_BUCKETS_NUM: usize = 100;

impl FutexTable {
    fn new() -> Self {
        let futex_buckets = (0..FUTEX_BUCKETS_NUM)
            .map(|_| Mutex::new(FutexBucket::new()))
            .collect();
        FutexTable { futex_buckets }
    }

    fn get_bucket(&self, key: &FutexKey) -> (usize, &Mutex<FutexBucket>) {
        let idx = key.hash() % self.futex_buckets.len();
        (idx, &self.futex_buckets[idx])
    }
}

impl FutexBucket {
    fn new() -> Self {
        FutexBucket {
            futex_items: RBTree::new(FutexItemAdapter::new()),
        }
    }

    fn find(&mut self, key: FutexKey) -> Option<Arc<FutexItem>> {
        let cursor = self.futex_items.find(&key);
        cursor.clone_pointer()
    }

    fn find_or_insert(&mut self, key: FutexKey) -> Arc<FutexItem> {
        if let Some(item) = self.find(key) {
            return item;
        }

        let item = Arc::new(FutexItem {
            link: RBTreeAtomicLink::new(),
            key,
            wait_list: WaitList::new(),
        });

        self.futex_items.insert(item.clone());

        item
    }
}

pub async fn futex_wait(
    uaddr: usize,
    pid: Option<u32>,
    expected_val: u32,
    _timeout: Option<u32>,
) -> KResult<()> {
    let futex_key = FutexKey::new(uaddr, pid);

    let futex_item = {
        let (_, futex_bucket_ref) = FUTEX_TABLE.get_bucket(&futex_key);
        let mut futex_bucket = futex_bucket_ref.lock().await;

        let val = UserPointer::new(uaddr as *const u32)?.read()?;

        if val != expected_val {
            return Err(EAGAIN);
        }

        futex_bucket.find_or_insert(futex_key)
    };

    let mut wait = pin!(futex_item.wait_list.prepare_to_wait());
    wait.as_mut().add_to_wait_list();
    wait.await;
    Ok(())
}

pub async fn futex_wake(uaddr: usize, pid: Option<u32>, max_wake_count: u32) -> KResult<usize> {
    let futex_key = FutexKey::new(uaddr, pid);

    let (_, futex_bucket_ref) = FUTEX_TABLE.get_bucket(&futex_key);
    let mut futex_bucket = futex_bucket_ref.lock().await;

    if let Some(futex_item) = futex_bucket.find(futex_key) {
        let mut count = 0;
        loop {
            let not_empty = futex_item.wait_list.notify_one();
            if not_empty == false || count == max_wake_count {
                break;
            }
            count += 1;
        }
        return Ok(count as usize);
    }

    Ok(0)
}

// should make sure that two keys in different buckets
// make the lock order to avoid deadlocks
async fn double_lock_bucket(
    futex_key0: FutexKey,
    futex_key1: FutexKey,
) -> (
    MutexGuard<'static, FutexBucket>,
    MutexGuard<'static, FutexBucket>,
) {
    let (bucket_idx0, bucket_ref0) = FUTEX_TABLE.get_bucket(&futex_key0);
    let (bucket_idx1, bucket_ref1) = FUTEX_TABLE.get_bucket(&futex_key1);

    if bucket_idx0 < bucket_idx1 {
        let bucket0 = bucket_ref0.lock().await;
        let bucket1 = bucket_ref1.lock().await;
        (bucket0, bucket1)
    } else {
        let bucket1 = bucket_ref1.lock().await;
        let bucket0 = bucket_ref0.lock().await;
        (bucket0, bucket1)
    }
}

async fn futex_requeue(
    uaddr: usize,
    pid: Option<u32>,
    wake_count: u32,
    requeue_uaddr: usize,
    requeue_count: u32,
) -> KResult<usize> {
    let futex_key = FutexKey::new(uaddr, pid);
    let futex_requeue_key = FutexKey::new(requeue_uaddr, pid);

    let (bucket_idx0, bucket_ref0) = FUTEX_TABLE.get_bucket(&futex_key);
    let (bucket_idx1, bucket_ref1) = FUTEX_TABLE.get_bucket(&futex_requeue_key);

    if bucket_idx0 == bucket_idx1 {
        // If the keys are the same, we can just wake up the waiters.
        return futex_wake(uaddr, pid, wake_count).await;
    }

    let (futex_bucket, futex_requeue_bucket) =
        double_lock_bucket(futex_key, futex_requeue_key).await;

    todo!()
}

// The purpose of the robust futex list is to ensure that if a thread
// accidentally fails to unlock a futex before terminating or calling
// execve(2), another thread that is waiting on that futex is
// notified that the former owner of the futex has died.  This
// notification consists of two pieces: the FUTEX_OWNER_DIED bit is
// set in the futex word, and the kernel performs a futex(2)
// FUTEX_WAKE operation on one of the threads waiting on the futex.
// https://man7.org/linux/man-pages/man2/get_robust_list.2.html

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RobustList {
    next: usize, // Pointer to the next RobustList entry
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RobustListHead {
    robust_list: RobustList,
    futex_offset: isize,
    list_op_pending: usize,
}

impl RobustListHead {
    fn futex_addr(&self, entry_ptr: usize) -> usize {
        (self.futex_offset + entry_ptr as isize) as usize
    }

    pub async fn wake_all(&self) -> KResult<()> {
        let end_ptr = self.robust_list.next;
        let mut entry_ptr = end_ptr;

        if entry_ptr == 0 {
            return Ok(());
        }

        loop {
            // Wake up the futex at the entry_ptr address.
            let futex_addr = self.futex_addr(entry_ptr);
            futex_wake(futex_addr, None, usize::MAX as u32).await?;

            // Move to the next entry in the robust list.
            let robust_list = UserPointer::new(entry_ptr as *const RobustList)?.read()?;

            entry_ptr = robust_list.next;

            if entry_ptr == end_ptr || entry_ptr == 0 {
                break;
            }
        }

        if self.list_op_pending != 0 {
            // If there is a pending operation, we need to wake it up.
            let pending_futex_addr = self.futex_addr(self.list_op_pending);
            futex_wake(pending_futex_addr, None, usize::MAX as u32).await?;
        }

        Ok(())
    }
}
