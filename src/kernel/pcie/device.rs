use alloc::collections::btree_map::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::num::NonZero;
use core::ops::RangeInclusive;

use align_ext::AlignExt;
use eonix_mm::address::{Addr, PAddr, PRange};
use eonix_sync::Spin;

use super::header::{Bar, Command};
use super::{CommonHeader, Header};
use crate::kernel::mem::PhysAccess as _;

pub(super) static PCIE_DEVICES: Spin<BTreeMap<u32, Vec<Arc<PCIDevice>>>> =
    Spin::new(BTreeMap::new());

pub struct PCIDevice<'a> {
    segment_group: SegmentGroup,
    config_space: ConfigSpace,
    pub header: Header<'a>,
    pub vendor_id: u16,
    pub device_id: u16,
}

#[allow(unused)]
#[derive(Clone)]
pub struct SegmentGroup {
    id: usize,
    bus_range: RangeInclusive<u8>,
    base_address: PAddr,
}

#[allow(unused)]
#[derive(Clone)]
pub struct ConfigSpace {
    pub bus: u8,
    pub device: u8,
    pub function: u8,

    pub base: PAddr,
}

pub struct PciMemoryAllocator {
    start: u32,
    end: u32,
}

impl SegmentGroup {
    pub fn new(id: usize, bus_start: u8, bus_end: u8, base_address: PAddr) -> Self {
        Self {
            id,
            bus_range: bus_start..=bus_end,
            base_address,
        }
    }

    #[cfg(target_arch = "x86_64")]
    pub fn from_entry(entry: &acpi::mcfg::PciConfigEntry) -> Self {
        Self {
            id: entry.segment_group as usize,
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
                        (0..8).map(move |function| {
                            self.get_conf_space(bus, device, function).unwrap()
                        })
                    })
                    .flatten()
            })
            .flatten()
    }

    pub fn get_conf_space(&self, bus: u8, device: u8, function: u8) -> Option<ConfigSpace> {
        if self.bus_range.contains(&bus) {
            Some(ConfigSpace {
                bus,
                device,
                function,
                base: self.base_address
                    + ((bus as usize) << 20)
                    + ((device as usize) << 15)
                    + ((function as usize) << 12),
            })
        } else {
            None
        }
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
        Arc::new(PCIDevice {
            segment_group,
            config_space,
            vendor_id: header.vendor_id,
            device_id: header.device_id,
            header,
        })
    }

    pub fn vendor_device(&self) -> u32 {
        (self.vendor_id as u32) << 16 | self.device_id as u32
    }
}

impl PCIDevice<'_> {
    pub fn configure_io(&self, allocator: &mut PciMemoryAllocator) {
        self.header
            .command()
            .clear(Command::IO_ACCESS_ENABLE | Command::MEMORY_ACCESS_ENABLE);

        if let Header::Endpoint(header) = self.header {
            for mut bar in header.bars().iter() {
                match bar.get() {
                    Bar::MemoryMapped32 { base: None, size } => bar.set(Bar::MemoryMapped32 {
                        base: Some(
                            allocator
                                .allocate(size as usize)
                                .expect("Failed to allocate BAR memory"),
                        ),
                        size,
                    }),
                    Bar::MemoryMapped64 { base: None, size } => bar.set(Bar::MemoryMapped64 {
                        base: Some(
                            allocator
                                .allocate(size as usize)
                                .map(|base| NonZero::new(base.get() as u64))
                                .flatten()
                                .expect("Failed to allocate BAR memory"),
                        ),
                        size,
                    }),
                    _ => {}
                }
            }
        }

        self.header.command().set(
            Command::IO_ACCESS_ENABLE | Command::MEMORY_ACCESS_ENABLE | Command::BUS_MASTER_ENABLE,
        );
    }

    #[allow(unused)]
    pub fn config_space(&self) -> &ConfigSpace {
        &self.config_space
    }

    #[allow(unused)]
    pub fn segment_group(&self) -> &SegmentGroup {
        &self.segment_group
    }
}

impl PciMemoryAllocator {
    pub fn new(range: PRange) -> Self {
        let start = range.start().addr() as u32;
        let end = range.end().addr() as u32;

        Self { start, end }
    }

    pub fn allocate(&mut self, size: usize) -> Option<NonZero<u32>> {
        let size = size.next_power_of_two().try_into().ok()?;
        let real_start = self.start.align_up(size);

        if size == 0 || size > self.end - real_start {
            return None;
        }

        let base = self.start;
        self.start += size;

        eonix_log::println_trace!(
            feat: "trace_pci",
            "PciMemoryAllocator: Allocated {} bytes at {:#x}",
            size,
            base
        );

        NonZero::new(base)
    }
}
