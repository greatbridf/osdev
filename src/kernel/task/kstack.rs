use crate::kernel::mem::{
    paging::Page,
    phys::{CachedPP, PhysPtr},
};

use core::cell::UnsafeCell;

pub struct KernelStack {
    pages: Page,
    bottom: usize,
    sp: UnsafeCell<usize>,
}

pub struct KernelStackWriter<'lt> {
    sp: &'lt mut usize,
    prev_sp: usize,

    pub entry: unsafe extern "C" fn(),
    pub flags: usize,
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbp: usize,
    pub rbx: usize,
}

unsafe extern "C" fn __not_assigned_entry() {
    panic!("__not_assigned_entry called");
}

impl<'lt> KernelStackWriter<'lt> {
    fn new(sp: &'lt mut usize) -> Self {
        let prev_sp = *sp;

        Self {
            sp,
            entry: __not_assigned_entry,
            flags: 0,
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbp: 0,
            rbx: 0,
            prev_sp,
        }
    }

    /// `data` and current sp should have an alignment of 16 bytes.
    /// Otherwise, extra padding is added.
    pub fn write<T: Copy>(&mut self, data: T) {
        *self.sp -= core::mem::size_of::<T>();
        *self.sp &= !0xf; // Align to 16 bytes

        // SAFETY: `sp` is always valid.
        unsafe {
            (*self.sp as *mut T).write(data);
        }
    }

    pub fn get_current_sp(&self) -> usize {
        *self.sp
    }

    fn push(&mut self, val: usize) {
        *self.sp -= core::mem::size_of::<usize>();

        // SAFETY: `sp` is always valid.
        unsafe {
            (*self.sp as *mut usize).write(val);
        }
    }

    pub fn finish(mut self) {
        self.push(self.entry as usize);
        self.push(self.flags); // rflags
        self.push(self.r15); // r15
        self.push(self.r14); // r14
        self.push(self.r13); // r13
        self.push(self.r12); // r12
        self.push(self.rbp); // rbp
        self.push(self.rbx); // rbx
        self.push(0); // 0 for alignment
        self.push(self.prev_sp) // previous sp
    }
}

impl KernelStack {
    /// Kernel stack page order
    /// 7 for `2^7 = 128 pages = 512 KiB`
    const KERNEL_STACK_ORDER: u32 = 7;

    pub fn new() -> Self {
        let pages = Page::alloc_many(Self::KERNEL_STACK_ORDER);
        let bottom = pages.as_cached().offset(pages.len()).as_ptr::<u8>() as usize;

        Self {
            pages,
            bottom,
            sp: UnsafeCell::new(bottom),
        }
    }

    pub fn load_interrupt_stack(&self) {
        const TSS_RSP0: CachedPP = CachedPP::new(0x00000074);

        // TODO!!!: Make `TSS` a per cpu struct.
        // SAFETY: `TSS_RSP0` is always valid.
        unsafe {
            TSS_RSP0.as_ptr::<u64>().write_unaligned(self.bottom as u64);
        }
    }

    pub fn get_writer(&mut self) -> KernelStackWriter {
        KernelStackWriter::new(self.sp.get_mut())
    }

    /// Get a pointer to `self.sp` so we can use it in `context_switch()`.
    ///
    /// # Safety
    /// Save the pointer somewhere or pass it to a function that will use it is UB.
    pub unsafe fn get_sp_ptr(&self) -> *mut usize {
        self.sp.get()
    }
}
