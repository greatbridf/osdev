use super::{
    device::{PCIDevice, SegmentGroup, PCIE_DEVICES},
    error::PciError,
};
use crate::kernel::mem::PhysAccess as _;
use acpi::{AcpiHandler, PhysicalMapping};
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
    #[cfg(target_arch = "x86_64")]
    {
        use acpi::{AcpiTables, PciConfigRegions};

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
    }

    #[cfg(target_arch = "riscv64")]
    {
        use crate::kernel::constants::{EINVAL, ENOENT};
        use eonix_hal::arch_exported::fdt::FDT;
        use eonix_mm::address::PRange;

        let pcie_node = FDT.find_node("/soc/pci").ok_or(ENOENT)?;
        let bus_range = pcie_node.property("bus-range").ok_or(ENOENT)?;
        let reg = pcie_node.reg().ok_or(EINVAL)?.next().ok_or(EINVAL)?;

        let mmio_range =
            PRange::from(PAddr::from(reg.starting_address as usize)).grow(reg.size.ok_or(EINVAL)?);

        if bus_range.value.len() != 8 {
            Err(EINVAL)?;
        }

        let bus_start = u32::from_be_bytes(bus_range.value[..4].try_into().unwrap());
        let bus_end = u32::from_be_bytes(bus_range.value[4..].try_into().unwrap());

        if bus_start > u8::MAX as u32 || bus_end > u8::MAX as u32 || bus_start > bus_end {
            Err(EINVAL)?;
        }

        let bus_start = bus_start as u8;
        let bus_end = bus_end as u8;

        let segment_group = SegmentGroup::new(0, bus_start, bus_end, mmio_range.start());
        for config_space in segment_group.iter() {
            if let Some(header) = config_space.header() {
                let pci_device = PCIDevice::new(segment_group.clone(), config_space, header);

                PCIE_DEVICES.lock().insert(pci_device);
            }
        }
    }

    Ok(())
}
