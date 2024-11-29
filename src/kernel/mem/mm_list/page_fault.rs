use arch::InterruptContext;
use bindings::{PA_A, PA_ANON, PA_COW, PA_MMAP, PA_P, PA_RW};
use bitflags::bitflags;

use crate::kernel::mem::paging::{Page, PageBuffer};
use crate::kernel::mem::phys::{CachedPP, PhysPtr};
use crate::kernel::mem::{Mapping, VRange};
use crate::kernel::task::{ProcessList, Signal, Thread};
use crate::prelude::*;

use super::{MMList, VAddr};

bitflags! {
    pub struct PageFaultError: u64 {
        const Present = 0x0001;
        const Write = 0x0002;
        const User = 0x0004;
        const ReservedSet = 0x0008;
        const InstructionFetch = 0x0010;
        const ProtectionKey = 0x0020;
        const SGX = 0x8000;
    }
}

#[repr(C)]
struct FixEntry {
    start: u64,
    length: u64,
    jump_address: u64,
    op_type: u64,
}

impl MMList {
    fn handle_page_fault(
        &self,
        int_stack: &mut InterruptContext,
        addr: VAddr,
        error: PageFaultError,
    ) -> Result<(), Signal> {
        let inner = self.inner.lock();
        let area = match inner.areas.get(&VRange::from(addr)) {
            Some(area) => area,
            None => {
                if error.contains(PageFaultError::User) {
                    return Err(Signal::SIGBUS);
                } else {
                    try_page_fault_fix(int_stack, addr);
                    return Ok(());
                }
            }
        };

        // User access permission violation, check user access permission.
        if error.contains(PageFaultError::User | PageFaultError::Present) {
            if error.contains(PageFaultError::Write) && !area.permission.write {
                ProcessList::kill_current(Signal::SIGSEGV)
            }

            if error.contains(PageFaultError::InstructionFetch) && !area.permission.execute {
                ProcessList::kill_current(Signal::SIGSEGV)
            }
        }

        let pte = self
            .page_table
            .iter_user(VRange::new(addr.floor(), addr.floor() + 0x1000))
            .unwrap()
            .next()
            .expect("If we can find the mapped area, we should be able to find the PTE");

        let is_mapped = matches!(&area.mapping, Mapping::File(_));
        if !is_mapped && !error.contains(PageFaultError::Present) {
            try_page_fault_fix(int_stack, addr);
            return Ok(());
        }

        let mut pfn = pte.pfn();
        let mut attributes = pte.attributes();

        if attributes & PA_COW as usize != 0 {
            attributes &= !PA_COW as usize;
            if area.permission.write {
                attributes |= PA_RW as usize;
            } else {
                attributes &= !PA_RW as usize;
            }

            let page = unsafe { Page::take_pfn(pfn, 0) };
            if unsafe { page.load_refcount() } == 1 {
                // SAFETY: This is actually safe. If we read `1` here and we have `MMList` lock
                // held, there couldn't be neither other processes sharing the page, nor other
                // threads making the page COW at the same time.
                pte.set_attributes(attributes);
                core::mem::forget(page);
                return Ok(());
            }

            let new_page = Page::alloc_one();
            if attributes & PA_ANON as usize != 0 {
                new_page.zero();
            } else {
                new_page
                    .as_cached()
                    .as_mut_slice::<u8>(0x1000)
                    .copy_from_slice(CachedPP::new(pfn).as_slice(0x1000));
            }

            attributes &= !(PA_A | PA_ANON) as usize;

            pfn = new_page.into_pfn();
            pte.set(pfn, attributes);
        }

        // TODO: shared mapping
        if attributes & PA_MMAP as usize != 0 {
            attributes |= PA_P as usize;

            if let Mapping::File(mapping) = &area.mapping {
                let load_offset = addr.floor() - area.range().start();
                if load_offset < mapping.length {
                    // SAFETY: Since we are here, the `pfn` must refer to a valid buddy page.
                    let page = unsafe { Page::from_pfn(pfn, 0) };
                    let nread = mapping
                        .file
                        .read(
                            &mut PageBuffer::new(page.clone()),
                            mapping.offset + load_offset,
                        )
                        .map_err(|_| Signal::SIGBUS)?;

                    if nread < page.len() {
                        page.as_cached().as_mut_slice::<u8>(0x1000)[nread..].fill(0);
                    }

                    if mapping.length - load_offset < 0x1000 {
                        let length_to_end = mapping.length - load_offset;
                        page.as_cached().as_mut_slice::<u8>(0x1000)[length_to_end..].fill(0);
                    }
                }
                // Otherwise, the page is kept zero emptied.

                attributes &= !PA_MMAP as usize;
                pte.set_attributes(attributes);
            } else {
                panic!("Anonymous mapping should not be PA_MMAP");
            }
        }

        Ok(())
    }
}

extern "C" {
    static FIX_START: *const FixEntry;
    static FIX_END: *const FixEntry;
}

/// Try to fix the page fault by jumping to the `error` address.
///
/// Panic if we can't find the `ip` in the fix list.
fn try_page_fault_fix(int_stack: &mut InterruptContext, addr: VAddr) {
    let ip = int_stack.rip as u64;
    // TODO: Use `op_type` to fix.

    // SAFETY: `FIX_START` and `FIX_END` are defined in the linker script in `.rodata` section.
    let entries = unsafe {
        core::slice::from_raw_parts(
            FIX_START,
            (FIX_END as usize - FIX_START as usize) / size_of::<FixEntry>(),
        )
    };

    for entry in entries.iter() {
        if ip >= entry.start && ip < entry.start + entry.length {
            int_stack.rip = entry.jump_address as u64;
            return;
        }
    }

    kernel_page_fault_die(addr, ip as usize)
}

fn kernel_page_fault_die(vaddr: VAddr, ip: usize) -> ! {
    panic!(
        "Invalid kernel mode memory access to {:#8x} while executing the instruction at {:#8x}",
        vaddr.0, ip
    )
}

pub fn handle_page_fault(int_stack: &mut InterruptContext) {
    let error = PageFaultError::from_bits_truncate(int_stack.error_code);
    let vaddr = VAddr(arch::get_page_fault_address());

    let result = Thread::current()
        .process
        .mm_list
        .handle_page_fault(int_stack, vaddr, error);

    if let Err(signal) = result {
        println_debug!(
            "Page fault on {:#x} in user space at {:#x}",
            vaddr.0,
            int_stack.rip
        );
        ProcessList::kill_current(signal)
    }
}
