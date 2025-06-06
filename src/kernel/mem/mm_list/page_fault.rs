use super::{MMList, VAddr};
use crate::kernel::task::{Signal, Thread};
use eonix_hal::traits::fault::PageFaultErrorCode;
use eonix_mm::address::{AddrOps as _, VRange};
use eonix_mm::paging::PAGE_SIZE;
use eonix_runtime::task::Task;

#[repr(C)]
struct FixEntry {
    start: u64,
    length: u64,
    jump_address: u64,
    op_type: u64,
}

impl FixEntry {
    fn start(&self) -> VAddr {
        VAddr::from(self.start as usize)
    }

    fn end(&self) -> VAddr {
        VAddr::from((self.start + self.length) as usize)
    }

    fn range(&self) -> VRange {
        VRange::new(self.start(), self.end())
    }

    fn jump_address(&self) -> VAddr {
        VAddr::from(self.jump_address as usize)
    }

    fn entries() -> &'static [FixEntry] {
        extern "C" {
            static FIX_START: *const FixEntry;
            static FIX_END: *const FixEntry;
        }

        unsafe {
            // SAFETY: `FIX_START` and `FIX_END` are defined in the
            //         linker script in `.rodata` section.
            core::slice::from_raw_parts(
                FIX_START,
                (FIX_END as usize - FIX_START as usize) / size_of::<FixEntry>(),
            )
        }
    }
}

impl MMList {
    /// Handle a user page fault.
    pub async fn handle_user_page_fault(
        &self,
        addr: VAddr,
        error: PageFaultErrorCode,
    ) -> Result<(), Signal> {
        debug_assert!(
            error.contains(PageFaultErrorCode::UserAccess),
            "Kernel mode page fault happened in user space."
        );

        let inner = self.inner.borrow();
        let inner = inner.lock().await;

        let area = inner.areas.get(&VRange::from(addr)).ok_or(Signal::SIGBUS)?;

        // Check user access permission.
        if error.contains(PageFaultErrorCode::Write) && !area.permission.write {
            Err(Signal::SIGSEGV)?
        }

        if error.contains(PageFaultErrorCode::InstructionFetch) && !area.permission.execute {
            Err(Signal::SIGSEGV)?
        }

        let pte = inner
            .page_table
            .iter_user(VRange::from(addr.floor()).grow(PAGE_SIZE))
            .next()
            .expect("If we can find the mapped area, we should be able to find the PTE");

        area.handle(pte, addr.floor() - area.range().start())
            .map_err(|_| Signal::SIGBUS)
    }
}

/// Try to fix the page fault by jumping to the `error` address.
///
/// # Return
/// Returns the new program counter after fixing.
///
/// # Panic
/// Panics if we can't find the instruction causing the fault in the fix list.
fn try_page_fault_fix(pc: VAddr, addr: VAddr) -> VAddr {
    // TODO: Use `op_type` to fix.
    for entry in FixEntry::entries().iter() {
        if pc >= entry.start() && pc < entry.end() {
            return entry.jump_address();
        }
    }

    kernel_page_fault_die(addr, pc)
}

#[cold]
fn kernel_page_fault_die(vaddr: VAddr, pc: VAddr) -> ! {
    panic!(
        "Invalid kernel mode memory access to {:?} while executing the instruction at {:?}",
        vaddr, pc
    )
}

pub fn handle_kernel_page_fault(
    fault_pc: VAddr,
    addr: VAddr,
    error: PageFaultErrorCode,
) -> Option<VAddr> {
    debug_assert!(
        !error.contains(PageFaultErrorCode::UserAccess),
        "User mode page fault happened in kernel space."
    );

    debug_assert!(
        !error.contains(PageFaultErrorCode::InstructionFetch),
        "Kernel mode instruction fetch fault."
    );

    // TODO: Move this to `UserBuffer` handler since we shouldn'e get any page fault
    //       in the kernel except for the instructions in the fix list.

    let mms = &Thread::current().process.mm_list;
    let inner = mms.inner.borrow();
    let inner = Task::block_on(inner.lock());

    let area = match inner.areas.get(&VRange::from(addr)) {
        Some(area) => area,
        None => {
            return Some(try_page_fault_fix(fault_pc, addr));
        }
    };

    let pte = inner
        .page_table
        .iter_user(VRange::from(addr.floor()).grow(PAGE_SIZE))
        .next()
        .expect("If we can find the mapped area, we should be able to find the PTE");

    if let Err(_) = area.handle(pte, addr.floor() - area.range().start()) {
        return Some(try_page_fault_fix(fault_pc, addr));
    }

    None
}
