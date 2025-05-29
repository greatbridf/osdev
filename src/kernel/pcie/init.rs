use super::{
    device::{PCIDevice, SegmentGroup, PCIE_DEVICES},
    error::PciError,
};
use crate::kernel::mem::PhysAccess as _;
use acpi::{AcpiHandler, AcpiTables, PciConfigRegions, PhysicalMapping};
use eonix_mm::address::PAddr;

#[derive(Clone)]
struct AcpiHandlerImpl;

impl AcpiHandler for AcpiHandlerImpl {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let virtual_address = unsafe {
            // SAFETY: `physical_address` is guaranteed to be valid by the caller.
            PAddr::from_val(physical_address).as_ptr()
        };

        PhysicalMapping::new(physical_address, virtual_address, size, size, self.clone())
    }

    fn unmap_physical_region<T>(_: &PhysicalMapping<Self, T>) {}
}

pub fn init_pcie() -> Result<(), PciError> {
    let acpi_tables = unsafe {
        // SAFETY: Our impl should be correct.
        AcpiTables::search_for_rsdp_bios(AcpiHandlerImpl)?
    };

    let conf_regions = PciConfigRegions::new(&acpi_tables)?;
    for region in conf_regions.iter() {
        let segment_group = SegmentGroup::from_entry(&region);

        for config_space in segment_group.iter() {
            if let Some(header) = config_space.header() {
                let pci_device = PCIDevice::new(segment_group.clone(), config_space, header);

                PCIE_DEVICES.lock().insert(pci_device);
            }
        }
    }

    Ok(())
}
