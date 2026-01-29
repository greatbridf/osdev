use core::ptr::NonNull;

#[cfg(target_arch = "x86_64")]
use eonix_hal::arch_exported::io::Port8;
use eonix_hal::mm::ArchPhysAccess;
use eonix_mm::address::{PAddr, PhysAccess};

use super::SerialRegister;

#[cfg(target_arch = "x86_64")]
pub struct SerialIO {
    tx_rx: Port8,
    int_ena: Port8,
    int_ident: Port8,
    line_control: Port8,
    modem_control: Port8,
    line_status: Port8,
    modem_status: Port8,
    scratch: Port8,
}

#[cfg(target_arch = "x86_64")]
impl SerialRegister for Port8 {
    fn read(&self) -> u8 {
        self.read()
    }

    fn write(&self, data: u8) {
        self.write(data);
    }
}

#[cfg(target_arch = "x86_64")]
impl SerialIO {
    /// Creates a new `SerialIO` instance with the given physical address.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the provided `base` is a valid IO port
    /// base for the serial port. The caller must ensure that this port base is correct.
    pub unsafe fn new(base: u16) -> Self {
        Self {
            tx_rx: Port8::new(base),
            int_ena: Port8::new(base + 1),
            int_ident: Port8::new(base + 2),
            line_control: Port8::new(base + 3),
            modem_control: Port8::new(base + 4),
            line_status: Port8::new(base + 5),
            modem_status: Port8::new(base + 6),
            scratch: Port8::new(base + 7),
        }
    }

    pub fn tx_rx(&self) -> impl SerialRegister {
        self.tx_rx
    }

    pub fn int_ena(&self) -> impl SerialRegister {
        self.int_ena
    }

    pub fn int_ident(&self) -> impl SerialRegister {
        self.int_ident
    }

    pub fn line_control(&self) -> impl SerialRegister {
        self.line_control
    }

    pub fn modem_control(&self) -> impl SerialRegister {
        self.modem_control
    }

    pub fn line_status(&self) -> impl SerialRegister {
        self.line_status
    }

    #[allow(unused)]
    pub fn modem_status(&self) -> impl SerialRegister {
        self.modem_status
    }

    #[allow(unused)]
    pub fn scratch(&self) -> impl SerialRegister {
        self.scratch
    }
}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
pub struct SerialIO {
    base_addr: NonNull<u8>,
}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
unsafe impl Send for SerialIO {}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
unsafe impl Sync for SerialIO {}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
impl SerialRegister for NonNull<u8> {
    fn read(&self) -> u8 {
        // SAFETY: `self` is a valid pointer to the serial port register.
        let retval = unsafe { self.as_ptr().read_volatile() };

        #[cfg(target_arch = "loongarch64")]
        eonix_hal::fence::memory_barrier();

        retval
    }

    fn write(&self, data: u8) {
        // SAFETY: `self` is a valid pointer to the serial port register.
        unsafe { self.as_ptr().write_volatile(data) };

        #[cfg(target_arch = "loongarch64")]
        eonix_hal::fence::memory_barrier();
    }
}

#[cfg(any(target_arch = "riscv64", target_arch = "loongarch64"))]
impl SerialIO {
    /// Creates a new `SerialIO` instance with the given physical address.
    ///
    /// # Safety
    /// This function is unsafe because it assumes that the provided `base_addr` is a valid
    /// physical address for the serial port. The caller must ensure that this address is correct
    /// and that the memory at this address is accessible.
    pub unsafe fn new(base_addr: PAddr) -> Self {
        Self {
            base_addr: unsafe {
                // SAFETY: `base_addr` is a valid physical address for the serial port.
                ArchPhysAccess::as_ptr(base_addr)
            },
        }
    }

    pub fn tx_rx(&self) -> impl SerialRegister {
        self.base_addr
    }

    pub fn int_ena(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(1) }
    }

    pub fn int_ident(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(2) }
    }

    pub fn line_control(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(3) }
    }

    pub fn modem_control(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(4) }
    }

    pub fn line_status(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(5) }
    }

    #[allow(unused)]
    pub fn modem_status(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(6) }
    }

    #[allow(unused)]
    pub fn scratch(&self) -> impl SerialRegister {
        unsafe { self.base_addr.add(7) }
    }
}
