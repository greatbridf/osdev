use core::marker::PhantomData;
use core::num::NonZero;
use core::ops::{BitAnd, BitOr, Deref, Not};
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};

use bitflags::bitflags;
use eonix_hal::fence::memory_barrier;

pub trait BitFlag: Sized + Copy {
    type Value: BitAnd<Output = Self::Value>
        + BitOr<Output = Self::Value>
        + Not<Output = Self::Value>;

    fn from_bits(value: Self::Value) -> Self;
    fn into_bits(self) -> Self::Value;

    fn and(self, other: Self) -> Self {
        Self::from_bits(self.into_bits() & other.into_bits())
    }

    fn or(self, other: Self) -> Self {
        Self::from_bits(self.into_bits() | other.into_bits())
    }

    fn not(self) -> Self {
        Self::from_bits(!self.into_bits())
    }
}

pub struct Register<'a, T>
where
    T: BitFlag,
{
    register: &'a AtomicU16,
    _phantom: PhantomData<T>,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct Command: u16 {
        /// I/O Access Enable.
        const IO_ACCESS_ENABLE = 1 << 0;
        /// Memory Access Enable.
        const MEMORY_ACCESS_ENABLE = 1 << 1;
        /// Bus Master Enable.
        const BUS_MASTER_ENABLE = 1 << 2;
        /// Special Cycle Enable.
        const SPECIAL_CYCLE_ENABLE = 1 << 3;
        /// Memory Write and Invalidate Enable.
        const MEMORY_WRITE_AND_INVALIDATE_ENABLE = 1 << 4;
        /// Palette Snooping Enable.
        const PALETTE_SNOOPING_ENABLE = 1 << 5;
        /// Parity Error Response Enable.
        const PARITY_ERROR_RESPONSE_ENABLE = 1 << 6;
        /// SERR# Enable.
        const SERR_ENABLE = 1 << 8;
        /// Fast Back-to-Back Enable.
        const FAST_BACK_TO_BACK_ENABLE = 1 << 9;
        /// Interrupt Disable.
        const INTERRUPT_DISABLE = 1 << 10;
    }

    #[derive(Debug, Clone, Copy)]
    pub struct Status: u16 {
        /// Interrupt Status.
        const INTERRUPT_STATUS = 1 << 3;
        /// Capabilities List.
        const CAPABILITIES_LIST = 1 << 4;
        /// 66 MHz Capable.
        const SIXTY_SIX_MHZ_CAPABLE = 1 << 5;
        /// Fast Back-to-Back Capable.
        const FAST_BACK_TO_BACK_CAPABLE = 1 << 7;
        /// Master Data Parity Error.
        const MASTER_DATA_PARITY_ERROR = 1 << 8;
        /// Device Select Timing.
        const DEVICE_SELECT_TIMING = (1 << 9) | (1 << 10);
        /// Signaled Target Abort.
        const SIGNALLED_TARGET_ABORT = 1 << 11;
        /// Received Target Abort.
        const RECEIVED_TARGET_ABORT = 1 << 12;
        /// Received Master Abort.
        const RECEIVED_MASTER_ABORT = 1 << 13;
        /// Signaled System Error.
        const SIGNALLED_SYSTEM_ERROR = 1 << 14;
        /// Detected Parity Error.
        const DETECTED_PARITY_ERROR = 1 << 15;
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Bar {
    IOMapped {
        base: Option<NonZero<u32>>,
        size: u32,
    },
    MemoryMapped32 {
        base: Option<NonZero<u32>>,
        size: u32,
    },
    MemoryMapped64 {
        base: Option<NonZero<u64>>,
        size: u64,
    },
    None,
}

pub struct BarsEntry<'a> {
    reg1: &'a AtomicU32,
    reg2: Option<&'a AtomicU32>,
}

pub struct Bars<'a> {
    bars: &'a [AtomicU32],
}

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
    _bars: [AtomicU32; 6],
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

impl BitFlag for Command {
    type Value = u16;

    fn from_bits(value: Self::Value) -> Self {
        Command::from_bits_retain(value)
    }

    fn into_bits(self) -> Self::Value {
        self.bits()
    }
}

impl BitFlag for Status {
    type Value = u16;

    fn from_bits(value: Self::Value) -> Self {
        Status::from_bits_retain(value)
    }

    fn into_bits(self) -> Self::Value {
        self.bits()
    }
}

impl<T> Register<'_, T>
where
    T: BitFlag<Value = u16>,
{
    pub fn read(&self) -> T {
        memory_barrier();
        let value = self.register.load(Ordering::Relaxed);
        memory_barrier();

        T::from_bits(value)
    }

    pub fn write(&self, value: T) {
        memory_barrier();
        self.register.store(value.into_bits(), Ordering::Relaxed);
        memory_barrier();
    }

    pub fn set(&self, value: T) {
        memory_barrier();
        let current = self.read();
        self.write(current.or(value));
        memory_barrier();
    }

    pub fn clear(&self, value: T) {
        memory_barrier();
        let current = self.read();
        self.write(current.and(value.not()));
        memory_barrier();
    }
}

impl CommonHeader {
    pub fn command(&self) -> Register<'_, Command> {
        Register {
            register: unsafe { AtomicU16::from_ptr((&raw const self._command) as *mut u16) },
            _phantom: PhantomData,
        }
    }

    pub fn status(&self) -> Register<'_, Status> {
        Register {
            register: unsafe { AtomicU16::from_ptr((&raw const self._status) as *mut u16) },
            _phantom: PhantomData,
        }
    }
}

impl Bars<'_> {
    pub fn iter(&self) -> impl Iterator<Item = BarsEntry<'_>> + use<'_> {
        struct BarsIterator<'a> {
            bars: &'a [AtomicU32],
            pos: usize,
        }

        impl<'a> Iterator for BarsIterator<'a> {
            type Item = BarsEntry<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                if self.pos >= self.bars.len() {
                    return None;
                }

                let reg1 = &self.bars[self.pos];
                let is_64bit = (reg1.load(Ordering::Relaxed) & 4) == 4;

                let reg2 = if is_64bit {
                    self.pos += 1;
                    Some(&self.bars[self.pos])
                } else {
                    None
                };

                self.pos += 1;

                Some(BarsEntry { reg1, reg2 })
            }
        }

        BarsIterator {
            bars: self.bars,
            pos: 0,
        }
    }
}

impl BarsEntry<'_> {
    pub fn get(&self) -> Bar {
        let reg1_value = self.reg1.load(Ordering::Relaxed);
        let is_mmio = (reg1_value & 1) == 0;
        let is_64bit = (reg1_value & 4) == 4;

        if !is_mmio {
            self.reg1.store(!0, Ordering::Relaxed);
            memory_barrier();

            let size = NonZero::new(self.reg1.load(Ordering::Relaxed) & !0x3);
            let bar = size.map(|size| Bar::IOMapped {
                base: NonZero::new(reg1_value & !0x3),
                size: !size.get() + 1,
            });

            self.reg1.store(reg1_value, Ordering::Relaxed);
            memory_barrier();

            return bar.unwrap_or(Bar::None);
        }

        if is_64bit {
            let reg2 = self.reg2.expect("64-bit BARs must have a second register");

            let reg2_value = reg2.load(Ordering::Relaxed);

            self.reg1.store(!0, Ordering::Relaxed);
            reg2.store(!0, Ordering::Relaxed);
            memory_barrier();

            let size = (self.reg1.load(Ordering::Relaxed) as u64 & !0xf)
                | ((reg2.load(Ordering::Relaxed) as u64) << 32);
            let bar = NonZero::new(size).map(|size| Bar::MemoryMapped64 {
                base: NonZero::new((reg1_value as u64 & !0xf) | ((reg2_value as u64) << 32)),
                size: !size.get() + 1,
            });

            self.reg1.store(reg1_value, Ordering::Relaxed);
            reg2.store(reg2_value, Ordering::Relaxed);
            memory_barrier();

            bar.unwrap_or(Bar::None)
        } else {
            self.reg1.store(!0, Ordering::Relaxed);
            memory_barrier();

            let size = NonZero::new(self.reg1.load(Ordering::Relaxed) & !0xf);
            let bar = size.map(|size| Bar::MemoryMapped32 {
                base: NonZero::new(reg1_value & !0xf),
                size: !size.get() + 1,
            });

            self.reg1.store(reg1_value, Ordering::Relaxed);
            memory_barrier();

            bar.unwrap_or(Bar::None)
        }
    }

    pub fn set(&mut self, bar: Bar) {
        match bar {
            Bar::None => panic!("Cannot set a BAR to None"),
            Bar::IOMapped { base, .. } => {
                let old_value = self.reg1.load(Ordering::Relaxed);
                let base = base.map_or(0, NonZero::get) & !0x3;

                self.reg1.store((old_value & 0x3) | base, Ordering::Relaxed);
                memory_barrier();
            }
            Bar::MemoryMapped32 { base, .. } => {
                let old_value = self.reg1.load(Ordering::Relaxed);
                let base = base.map_or(0, NonZero::get) & !0xf;

                self.reg1.store((old_value & 0xf) | base, Ordering::Relaxed);
                memory_barrier();
            }
            Bar::MemoryMapped64 { base, .. } => {
                let reg2 = self.reg2.expect("64-bit BARs must have a second register");
                let old_value1 = self.reg1.load(Ordering::Relaxed);
                let old_value2 = reg2.load(Ordering::Relaxed);
                let base = base.map_or(0, NonZero::get) & !0xf;

                self.reg1
                    .store((old_value1 & 0xf) | base as u32, Ordering::Relaxed);
                reg2.store(old_value2 | (base >> 32) as u32, Ordering::Relaxed);
                memory_barrier();
            }
        }
    }
}

impl Endpoint {
    pub fn bars(&self) -> Bars<'_> {
        Bars { bars: &self._bars }
    }
}

impl Deref for Header<'_> {
    type Target = CommonHeader;

    fn deref(&self) -> &Self::Target {
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
