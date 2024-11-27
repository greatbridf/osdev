#![allow(dead_code)]

use crate::kernel::mem::paging::Page;
pub const VENDOR_INTEL: u16 = 0x8086;
pub const DEVICE_AHCI: u16 = 0x2922;

pub const PCI_REG_ABAR: usize = 0x05;

pub const GHC_IE: u32 = 0x00000002;
pub const GHC_AE: u32 = 0x80000000;

pub const ATA_DEV_BSY: u32 = 0x00000008;
pub const ATA_DEV_DRQ: u32 = 0x00000004;

pub const PORT_CMD_ST: u32 = 0x00000001;
pub const PORT_CMD_FRE: u32 = 0x00000010;
pub const PORT_CMD_FR: u32 = 0x00004000;
pub const PORT_CMD_CR: u32 = 0x00008000;

pub const PORT_IE_DHRE: u32 = 0x00000001;
pub const PORT_IE_UFE: u32 = 0x00000010;
pub const PORT_IE_INFE: u32 = 0x04000000;
pub const PORT_IE_IFE: u32 = 0x08000000;
pub const PORT_IE_HBDE: u32 = 0x10000000;
pub const PORT_IE_IBFE: u32 = 0x20000000;
pub const PORT_IE_TFEE: u32 = 0x40000000;

pub const PORT_IE_DEFAULT: u32 = PORT_IE_DHRE
    | PORT_IE_UFE
    | PORT_IE_INFE
    | PORT_IE_IFE
    | PORT_IE_HBDE
    | PORT_IE_IBFE
    | PORT_IE_TFEE;

pub const PORT_IS_DHRS: u32 = 0x00000001;
pub const PORT_IS_UFS: u32 = 0x00000010;
pub const PORT_IS_INFS: u32 = 0x04000000;
pub const PORT_IS_IFS: u32 = 0x08000000;
pub const PORT_IS_HBDS: u32 = 0x10000000;
pub const PORT_IS_IBFS: u32 = 0x20000000;
pub const PORT_IS_TFES: u32 = 0x40000000;

pub const PORT_IS_ERROR: u32 =
    PORT_IS_UFS | PORT_IS_INFS | PORT_IS_IFS | PORT_IS_HBDS | PORT_IS_IBFS;

/// A `CommandHeader` is used to send commands to the HBA device
///
/// # Access
///
/// `clear_busy_upon_ok` and `bytes_transferred` are volatile
///
#[repr(C)]
pub struct CommandHeader {
    // [0:4]: Command FIS length
    // [5]: ATAPI
    // [6]: Write
    // [7]: Prefetchable
    pub first: u8,

    // [0]: Reset
    // [1]: BIST
    // [2]: Clear busy upon ok
    // [3]: Reserved
    // [4:7]: Port multiplier
    pub second: u8,

    pub prdt_length: u16,
    pub bytes_transferred: u32,
    pub command_table_base: u64,

    pub _reserved: [u32; 4],
}

pub enum FisType {
    H2D = 0x27,
    D2H = 0x34,
    DMAAct = 0x39,
    DMASetup = 0x41,
    Data = 0x46,
    BIST = 0x58,
    PIOSetup = 0x5f,
    DevBits = 0xa1,
}

/// A `FISH2D` is a Host to Device FIS
///
#[repr(C)]
pub struct FISH2D {
    fis_type: FisType,

    // [0:3]: Port
    // [4:6]: Reserved
    // [7]: IsCommand
    shared: u8,

    command: u8,
    feature: u8,

    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,

    lba3: u8,
    lba4: u8,
    lba5: u8,
    feature_high: u8,

    count: u16,
    iso_command_completion: u8,
    control_register: u8,

    _reserved: [u8; 4],
}

impl FISH2D {
    pub fn setup(&mut self, cmd: u8, lba: u64, count: u16) {
        self.fis_type = FisType::H2D;
        self.shared = 0x80; // IsCommand
        self.command = cmd;
        self.feature = 0;

        self.lba0 = (lba & 0xff) as u8;
        self.lba1 = ((lba >> 8) & 0xff) as u8;
        self.lba2 = ((lba >> 16) & 0xff) as u8;
        self.device = 0x40; // LBA mode

        self.lba3 = ((lba >> 24) & 0xff) as u8;
        self.lba4 = ((lba >> 32) & 0xff) as u8;
        self.lba5 = ((lba >> 40) & 0xff) as u8;
        self.feature_high = 0;

        self.count = count;
        self.iso_command_completion = 0;
        self.control_register = 0;

        self._reserved = [0; 4];
    }
}

/// A `FISD2H` is a Device to Host FIS
///
#[repr(C)]
pub struct FISD2H {
    fis_type: FisType,

    /// [0:3]: Port
    /// [4:5]: Reserved
    /// [6]: Interrupt
    /// [7]: Reserved
    shared: u8,

    status: u8,
    error: u8,

    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,

    lba3: u8,
    lba4: u8,
    lba5: u8,
    _reserved1: u8,

    count: u16,

    _reserved2: [u8; 6],
}

/// A `FISPIOSetup` is a PIO Setup FIS
///
#[repr(C)]
pub struct FISPIOSetup {
    fis_type: FisType,

    /// [0:3]: Port
    /// [4]: Reserved
    /// [5]: Data transfer direction
    /// [6]: Interrupt
    /// [7]: Reserved
    shared: u8,

    status: u8,
    error: u8,

    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,

    lba3: u8,
    lba4: u8,
    lba5: u8,
    _reserved1: u8,

    count: u16,
    _reserved2: u8,
    new_status: u8,

    transfer_count: u16,
    _reserved3: [u8; 2],
}

/// A `ReceivedFis` is a FIS that the HBA device has received
///
#[repr(C)]
pub struct ReceivedFis {
    fis_dma_setup: [u8; 32], // we don't care about it for now

    fis_pio: FISPIOSetup,
    padding1: [u8; 12],

    fis_reg: FISD2H,
    padding2: [u8; 4],

    fissdb: [u8; 8],

    ufis: [u8; 64],

    reserved: [u8; 96],
}

/// A `PRDTEntry` is a Physical Region Descriptor Table entry
///
#[repr(C)]
pub struct PRDTEntry {
    base: u64,
    _reserved1: u32,

    /// [0:21]: Byte count
    /// [22:30]: Reserved
    /// [31]: Interrupt on completion
    shared: u32,
}

impl PRDTEntry {
    pub fn setup(&mut self, page: &Page) {
        self.base = page.as_phys() as u64;
        self._reserved1 = 0;

        self.shared = 0x80000000 | (page.len() as u32 & 0x3fffff);
    }
}

// A CommandTable consists of a Command FIS, ATAPI Command, and PRDT entries
// The Command FIS is always at the beginning of the table
// Add 44 + 16 + 48 = 108 bytes will be the PRDT entries
