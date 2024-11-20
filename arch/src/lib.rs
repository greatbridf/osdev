#![no_std]

pub mod vm {
    pub fn invlpg(vaddr: usize) {
        x86_64::vm::invlpg(vaddr)
    }

    pub fn invlpg_all() {
        x86_64::vm::invlpg_all()
    }

    pub fn current_page_table() -> usize {
        x86_64::vm::get_cr3()
    }

    pub fn switch_page_table(pfn: usize) {
        x86_64::vm::set_cr3(pfn)
    }
}

pub mod task {
    #[inline(always)]
    pub fn halt() {
        x86_64::task::halt()
    }

    #[inline(always)]
    pub fn pause() {
        x86_64::task::pause()
    }

    #[inline(always)]
    pub fn freeze() -> ! {
        x86_64::task::freeze()
    }

    /// Switch to the `next` task. `IF` state is also switched.
    ///
    /// This function should only be used to switch between tasks that do not need SMP synchronization.
    ///
    /// # Arguments
    /// * `current_task_sp` - Pointer to the stack pointer of the current task.
    /// * `next_task_sp` - Pointer to the stack pointer of the next task.
    #[inline(always)]
    pub fn context_switch_light(current_task_sp: *mut usize, next_task_sp: *mut usize) {
        x86_64::task::context_switch_light(current_task_sp, next_task_sp);
    }
}

pub mod interrupt {
    #[inline(always)]
    pub fn enable() {
        x86_64::interrupt::enable()
    }

    #[inline(always)]
    pub fn disable() {
        x86_64::interrupt::disable()
    }
}

pub mod io {
    #[inline(always)]
    pub fn inb(port: u16) -> u8 {
        x86_64::io::inb(port)
    }

    #[inline(always)]
    pub fn outb(port: u16, data: u8) {
        x86_64::io::outb(port, data)
    }

    #[inline(always)]
    pub fn inw(port: u16) -> u16 {
        x86_64::io::inw(port)
    }

    #[inline(always)]
    pub fn outw(port: u16, data: u16) {
        x86_64::io::outw(port, data)
    }

    #[inline(always)]
    pub fn inl(port: u16) -> u32 {
        x86_64::io::inl(port)
    }

    #[inline(always)]
    pub fn outl(port: u16, data: u32) {
        x86_64::io::outl(port, data)
    }
}

pub use x86_64;
