use core::sync::atomic::{AtomicU32, Ordering};

use crate::kernel::constants::EFAULT;
use alloc::{
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use eonix_sync::{Mutex, Spin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSpeed {
    SpeedUnknown,
    Speed10M,
    Speed100M,
    Speed1000M,
}

pub type Mac = [u8; 6];

#[allow(dead_code)]
pub trait Netdev: Send {
    fn up(&mut self) -> Result<(), u32>;
    fn send(&mut self, data: &[u8]) -> Result<(), u32>;
    fn fire(&mut self) -> Result<(), u32>;

    fn link_status(&self) -> LinkStatus;
    fn link_speed(&self) -> LinkSpeed;
    fn mac(&self) -> Mac;

    fn id(&self) -> u32;
}

impl PartialEq for dyn Netdev {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for dyn Netdev {}

impl PartialOrd for dyn Netdev {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for dyn Netdev {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

static NETDEVS_ID: AtomicU32 = AtomicU32::new(0);
static NETDEVS: Spin<BTreeMap<u32, Arc<Mutex<dyn Netdev>>>> = Spin::new(BTreeMap::new());

pub fn alloc_id() -> u32 {
    NETDEVS_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn register_netdev(netdev: impl Netdev + 'static) -> Result<Arc<Mutex<dyn Netdev>>, u32> {
    match NETDEVS.lock().entry(netdev.id()) {
        Entry::Vacant(entry) => {
            let netdev = Arc::new(Mutex::new(netdev));
            entry.insert(netdev.clone());
            Ok(netdev)
        }
        Entry::Occupied(_) => Err(EFAULT),
    }
}

pub fn get_netdev(id: u32) -> Option<Arc<Mutex<dyn Netdev>>> {
    NETDEVS.lock().get(&id).map(|netdev| netdev.clone())
}
