use super::{
    device::{PCIDevice, PCIE_DEVICES},
    error::PciError,
};
use crate::{kernel::constants::EEXIST, KResult};
use alloc::{
    collections::btree_map::{self, BTreeMap},
    sync::Arc,
};
use eonix_sync::Spin;

static PCIE_DRIVERS: Spin<BTreeMap<u32, Arc<dyn PCIDriver>>> = Spin::new(BTreeMap::new());

pub trait PCIDriver: Send + Sync {
    fn vendor_id(&self) -> u16;
    fn device_id(&self) -> u16;

    fn handle_device(&self, device: Arc<PCIDevice<'static>>) -> Result<(), PciError>;
}

pub fn register_driver(driver: impl PCIDriver + 'static) -> KResult<()> {
    let index = (driver.vendor_id() as u32) << 16 | driver.device_id() as u32;

    let driver = Arc::new(driver);

    match PCIE_DRIVERS.lock().entry(index) {
        btree_map::Entry::Vacant(vacant_entry) => vacant_entry.insert(driver.clone()),
        btree_map::Entry::Occupied(_) => Err(EEXIST)?,
    };

    let device = PCIE_DEVICES.lock().find(&index).clone_pointer();
    if let Some(device) = device {
        driver.handle_device(device)?;
    };

    Ok(())
}
