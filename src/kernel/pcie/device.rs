use super::{CommonHeader, Header};
use crate::{kernel::mem::PhysAccess as _, sync::fence::memory_barrier};
use acpi::mcfg::PciConfigEntry;
use alloc::sync::Arc;
use core::{ops::RangeInclusive, sync::atomic::Ordering};
use eonix_mm::address::PAddr;
use eonix_sync::{LazyLock, Spin};
use intrusive_collections::{intrusive_adapter, KeyAdapter, RBTree, RBTreeAtomicLink};

pub(super) static PCIE_DEVICES: LazyLock<Spin<RBTree<PCIDeviceAdapter>>> =
    LazyLock::new(|| Spin::new(RBTree::new(PCIDeviceAdapter::new())));

intrusive_adapter!(
    pub PCIDeviceAdapter = Arc<PCIDevice<'static>> : PCIDevice { link: RBTreeAtomicLink }
);

#[allow(dead_code)]
pub struct PCIDevice<'a> {
    link: RBTreeAtomicLink,
    segment_group: SegmentGroup,
    config_space: ConfigSpace,
    pub header: Header<'a>,
    pub vendor_id: u16,
    pub device_id: u16,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct SegmentGroup {
    id: u16,
    bus_range: RangeInclusive<u8>,
    base_address: PAddr,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct ConfigSpace {
    bus: u8,
    device: u8,
    function: u8,

    base: PAddr,
}

impl SegmentGroup {
    pub fn from_entry(entry: &PciConfigEntry) -> Self {
        Self {
            id: entry.segment_group,
            bus_range: entry.bus_range.clone(),
            base_address: PAddr::from(entry.physical_address),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = ConfigSpace> + use<'_> {
        self.bus_range
            .clone()
            .map(move |bus| {
                (0..32)
                    .map(move |device| {
                        (0..8).map(move |function| ConfigSpace {
                            bus,
                            device,
                            function,
                            base: self.base_address
                                + ((bus as usize) << 20)
                                + ((device as usize) << 15)
                                + ((function as usize) << 12),
                        })
                    })
                    .flatten()
            })
            .flatten()
    }
}

impl ConfigSpace {
    pub fn header<'a>(&self) -> Option<Header<'a>> {
        let common_header = unsafe {
            // SAFETY: `self.base` is guaranteed to be pointing to a valid
            //         configuration space area.
            self.base.as_ptr::<CommonHeader>().as_ref()
        };

        if common_header.vendor_id == 0xffff {
            return None;
        }

        // Bit 7 represents whether the device has multiple functions.
        let header_type = common_header.header_type & !(1 << 7);

        match header_type {
            0 => Some(Header::Endpoint(unsafe {
                // SAFETY: `self.base` is guaranteed to be pointing to a valid
                //         configuration space area.
                self.base.as_ptr().as_ref()
            })),
            1 | 2 => unimplemented!("Header type 1 and 2"),
            _ => Some(Header::Unknown(unsafe {
                // SAFETY: `self.base` is guaranteed to be pointing to a valid
                //         configuration space area.
                self.base.as_ptr().as_ref()
            })),
        }
    }
}

impl PCIDevice<'static> {
    pub fn new(
        segment_group: SegmentGroup,
        config_space: ConfigSpace,
        header: Header<'static>,
    ) -> Arc<Self> {
        let common_header = header.common_header();

        Arc::new(PCIDevice {
            link: RBTreeAtomicLink::new(),
            segment_group,
            config_space,
            vendor_id: common_header.vendor_id,
            device_id: common_header.device_id,
            header,
        })
    }
}

#[allow(dead_code)]
impl PCIDevice<'_> {
    pub fn enable_bus_mastering(&self) {
        let header = self.header.common_header();
        header.command().fetch_or(0x04, Ordering::Relaxed);

        memory_barrier();
    }
}

impl<'a> KeyAdapter<'a> for PCIDeviceAdapter {
    type Key = u32;

    fn get_key(
        &self,
        value: &'a <Self::PointerOps as intrusive_collections::PointerOps>::Value,
    ) -> Self::Key {
        ((value.vendor_id as u32) << 16) | value.device_id as u32
    }
}
