//! External interrupt support for LoongArch64.
//!
//! On the QEMU `virt` machine the external interrupt path is:
//!
//! ```text
//! device (UART, ...) -> PCH-PIC line -> EIOINTC vector -> CPU HWI line
//! ```
//!
//! The PCH-PIC is a normal MMIO device, while the EIOINTC (EXTIOI) lives in
//! the IOCSR address space and must be accessed with `iocsrrd`/`iocsrwr`.
//!
//! EIOINTC vectors are grouped: each group of 32 vectors is routed to one of
//! the four CPU hardware interrupt lines (HWI0..HWI3, i.e. CPU interrupt
//! numbers 2..5).  We route every group to HWI1, which is the line the device
//! tree advertises for the interrupt controller (`interrupts = <3>`), and
//! every vector to CPU 0.

use eonix_mm::address::{PAddr, PhysAccess};
use loongArch64::iocsr;

use super::mm::ArchPhysAccess;

/// Base address of the PCH-PIC (normal MMIO).
const PCH_PIC_BASE: usize = 0x1000_0000;

const PCH_PIC_INT_MASK: usize = 0x020;
const PCH_PIC_HTMSI_VEC: usize = 0x200;

/// Number of interrupt lines handled by the PCH-PIC.
const PCH_PIC_LINES: usize = 64;

/// Number of EIOINTC vectors wired to PCH-PIC lines.
const PCH_PIC_VECTORS: usize = 32;

/// EIOINTC registers, in the IOCSR address space.
const EIOINTC_IPMAP: usize = 0x14c0;
const EIOINTC_ENABLE: usize = 0x1600;
const EIOINTC_COREISR: usize = 0x1800;
const EIOINTC_COREMAP: usize = 0x1c00;

const EIOINTC_VECTORS: usize = 256;
const EIOINTC_GROUP_SIZE: usize = 32;
const EIOINTC_GROUPS: usize = EIOINTC_VECTORS / EIOINTC_GROUP_SIZE;

/// CPU interrupt line all EIOINTC vectors are routed to.
///
/// This is the `ipnum` passed to the EIOINTC, whose outputs are connected to
/// CPU interrupt inputs `ipnum + 2` (HWI0 = 2).
const EIOINTC_IPNUM: usize = 1;

fn pch_read32(offset: usize) -> u32 {
    unsafe {
        // SAFETY: The PCH-PIC registers are memory mapped at a well-known
        //         physical address on the QEMU virt platform.
        ArchPhysAccess::as_ptr::<u32>(PAddr::from(PCH_PIC_BASE + offset))
            .as_ptr()
            .read_volatile()
    }
}

fn pch_write32(offset: usize, value: u32) {
    unsafe {
        // SAFETY: See `pch_read32`.
        ArchPhysAccess::as_ptr::<u32>(PAddr::from(PCH_PIC_BASE + offset))
            .as_ptr()
            .write_volatile(value)
    }
}

fn pch_write8(offset: usize, value: u8) {
    unsafe {
        // SAFETY: See `pch_read32`.
        ArchPhysAccess::as_ptr::<u8>(PAddr::from(PCH_PIC_BASE + offset))
            .as_ptr()
            .write_volatile(value)
    }
}

fn eiointc_enable_vector(vector: usize) {
    let offset =
        EIOINTC_ENABLE + (vector / EIOINTC_GROUP_SIZE) * size_of::<u32>();
    let bit = 1u32 << (vector % EIOINTC_GROUP_SIZE);

    iocsr::iocsr_write_w(offset, iocsr::iocsr_read_w(offset) | bit);
}

fn eiointc_clear_vector(vector: usize) {
    let offset =
        EIOINTC_COREISR + (vector / EIOINTC_GROUP_SIZE) * size_of::<u32>();
    let bit = 1u32 << (vector % EIOINTC_GROUP_SIZE);

    // Writing one acknowledges (clears) the pending bit.
    iocsr::iocsr_write_w(offset, bit);
}

/// Initialize the external interrupt controllers.
///
/// Must be called once per CPU before interrupts are enabled.
pub fn init() {
    // Route all 256 vectors to HWI1.  The IPMAP register contains one byte
    // per 32-vector group, holding a bitmask of the target `ipnum`.
    let ipmap = 0x01u32 << EIOINTC_IPNUM;
    let ipmap = ipmap * 0x0101_0101;
    for group in 0..(EIOINTC_VECTORS / EIOINTC_GROUP_SIZE / 4) {
        iocsr::iocsr_write_w(EIOINTC_IPMAP + group * size_of::<u32>(), ipmap);
    }

    // Route all vectors to CPU 0.  COREMAP stores four bytes (one vector
    // each) per 32-bit register; a byte holds a bitmask of target CPUs.
    for word in 0..(EIOINTC_VECTORS / 4) {
        iocsr::iocsr_write_w(
            EIOINTC_COREMAP + word * size_of::<u32>(),
            0x0101_0101,
        );
    }
}

/// Enable an external interrupt line.
///
/// `irq` is the PCH-PIC line number, which is also the EIOINTC vector number
/// for the lower 32 vectors.
pub fn enable_irq(irq: usize) {
    assert!(irq < PCH_PIC_LINES, "PCH-PIC line out of range");

    // Route this PCH-PIC line to the EIOINTC vector with the same number
    // before unmasking it, so the first interrupt already lands on the
    // correct vector.
    pch_write8(PCH_PIC_HTMSI_VEC + irq, irq as u8);

    if irq < PCH_PIC_VECTORS {
        eiointc_clear_vector(irq);
        eiointc_enable_vector(irq);
    }

    // Unmask the line in the PCH-PIC.
    let offset = PCH_PIC_INT_MASK + (irq / 32) * size_of::<u32>();
    let bit = 1u32 << (irq % 32);
    pch_write32(offset, pch_read32(offset) & !bit);
}

/// Handle pending external interrupts by invoking `handler` for each of them.
pub fn dispatch_irq(handler: fn(usize)) {
    for group in 0..EIOINTC_GROUPS {
        let offset = EIOINTC_COREISR + group * size_of::<u32>();
        let mut pending = iocsr::iocsr_read_w(offset);

        if pending == 0 {
            continue;
        }

        // Acknowledge before dispatching, like Linux does.  The peripheral
        // keeps the line asserted until its handler has serviced it, and any
        // new event after that re-raises the interrupt.
        iocsr::iocsr_write_w(offset, pending);

        while pending != 0 {
            let bit = pending.trailing_zeros() as usize;
            pending &= pending - 1;

            let vector = group * EIOINTC_GROUP_SIZE + bit;

            // Vectors 0..32 are wired to the PCH-PIC.  Higher vectors belong
            // to the PCI MSI controller, which we do not support yet.
            if vector < PCH_PIC_VECTORS {
                handler(vector);
            }
        }
    }
}
