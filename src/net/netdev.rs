use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use spin::Mutex;

use crate::bindings::root::EFAULT;

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

pub trait Netdev {
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

static mut NETDEVS_ID: Mutex<u32> = Mutex::new(0);
static mut NETDEVS: Mutex<BTreeMap<u32, Arc<Mutex<dyn Netdev>>>> =
    Mutex::new(BTreeMap::new());

pub fn alloc_id() -> u32 {
    let mut id = unsafe { NETDEVS_ID.lock() };
    let retval = *id;

    *id += 1;
    retval
}

pub fn register_netdev(
    netdev: impl Netdev + 'static,
) -> Result<Arc<Mutex<dyn Netdev>>, u32> {
    let devid = netdev.id();

    let mut netdevs = unsafe { NETDEVS.lock() };

    use alloc::collections::btree_map::Entry;
    match netdevs.entry(devid) {
        Entry::Vacant(entry) => {
            let netdev = Arc::new(Mutex::new(netdev));
            entry.insert(netdev.clone());
            Ok(netdev)
        }
        Entry::Occupied(_) => Err(EFAULT),
    }
}

pub fn get_netdev(id: u32) -> Option<Arc<Mutex<dyn Netdev>>> {
    let netdevs = unsafe { NETDEVS.lock() };

    netdevs.get(&id).map(|netdev| netdev.clone())
}
