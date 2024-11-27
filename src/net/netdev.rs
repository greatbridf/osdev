use alloc::{collections::btree_map::BTreeMap, sync::Arc};

use crate::{bindings::root::EFAULT, prelude::*};

use lazy_static::lazy_static;

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

lazy_static! {
    static ref NETDEVS_ID: Spin<u32> = Spin::new(0);
    static ref NETDEVS: Spin<BTreeMap<u32, Arc<Mutex<dyn Netdev>>>> =
        Spin::new(BTreeMap::new());
}

pub fn alloc_id() -> u32 {
    let mut id = NETDEVS_ID.lock();
    let retval = *id;

    *id += 1;
    retval
}

pub fn register_netdev(
    netdev: impl Netdev + 'static,
) -> Result<Arc<Mutex<dyn Netdev>>, u32> {
    let devid = netdev.id();

    let mut netdevs = NETDEVS.lock();

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
    NETDEVS.lock().get(&id).map(|netdev| netdev.clone())
}
