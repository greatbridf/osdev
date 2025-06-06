use core::{mem::offset_of, ops::Deref, sync::atomic::AtomicU32};

#[repr(C)]
pub struct CommonHeader {
    pub vendor_id: u16,
    pub device_id: u16,
    /// Command Register.
    ///
    /// Use `command()` instead of accessing this field.
    _command: u16,
    /// Status Register.
    ///
    /// Use `command()` instead of accessing this field.
    _status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class: u8,
    pub cache_line_size: u8,
    pub latency_timer: u8,
    pub header_type: u8,
    pub bist: u8,
}

#[repr(C)]
pub struct Endpoint {
    _header: CommonHeader,
    pub bars: [u32; 6],
    pub cardbus_cis_pointer: u32,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub expansion_rom_address: u32,
    pub capabilities_pointer: u8,
    _reserved: [u8; 7],
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub min_grant: u8,
    pub max_latency: u8,
}

pub enum Header<'a> {
    Unknown(&'a CommonHeader),
    Endpoint(&'a Endpoint),
}

#[allow(dead_code)]
impl CommonHeader {
    pub fn command(&self) -> &AtomicU32 {
        unsafe {
            AtomicU32::from_ptr(
                (&raw const *self)
                    .byte_offset(offset_of!(CommonHeader, _command) as isize)
                    .cast::<u32>() as *mut u32,
            )
        }
    }

    pub fn status(&self) -> &AtomicU32 {
        unsafe {
            AtomicU32::from_ptr(
                (&raw const *self)
                    .byte_offset(offset_of!(CommonHeader, _status) as isize)
                    .cast::<u32>() as *mut u32,
            )
        }
    }
}

impl Header<'_> {
    pub fn common_header(&self) -> &CommonHeader {
        match self {
            Header::Unknown(header) => header,
            Header::Endpoint(header) => &header,
        }
    }
}

impl Deref for Endpoint {
    type Target = CommonHeader;

    fn deref(&self) -> &Self::Target {
        &self._header
    }
}
