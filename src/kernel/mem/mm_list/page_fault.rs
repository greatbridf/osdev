use super::{MMList, VAddr};
use crate::kernel::mem::Mapping;
use crate::kernel::task::{ProcessList, Signal, Thread};
use crate::prelude::*;
use arch::InterruptContext;
use bitflags::bitflags;
use eonix_mm::address::{AddrOps as _, VRange};
use eonix_mm::paging::PAGE_SIZE;
use eonix_runtime::task::Task;

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
        let inner = self.inner.borrow();
        let inner = Task::block_on(inner.lock());

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

        let pte = inner
            .page_table
            .iter_user(VRange::from(addr.floor()).grow(PAGE_SIZE))
            .next()
            .expect("If we can find the mapped area, we should be able to find the PTE");

        let is_mapped = matches!(&area.mapping, Mapping::File(_));
        if !is_mapped && !error.contains(PageFaultError::Present) {
            try_page_fault_fix(int_stack, addr);
            return Ok(());
        }

        area.handle(pte, addr.floor() - area.range().start())
            .map_err(|_| Signal::SIGBUS)
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
        "Invalid kernel mode memory access to {:?} while executing the instruction at {:#8x}",
        vaddr, ip
    )
}

pub fn handle_page_fault(int_stack: &mut InterruptContext) {
    let error = PageFaultError::from_bits_truncate(int_stack.error_code);
    let vaddr = arch::get_page_fault_address();

    let result = Thread::current()
        .process
        .mm_list
        .handle_page_fault(int_stack, vaddr, error);

    if let Err(signal) = result {
        println_debug!(
            "Page fault on {:?} in user space at {:#x}",
            vaddr,
            int_stack.rip
        );
        ProcessList::kill_current(signal)
    }
}
