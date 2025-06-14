use core::ptr::{read_volatile, write_volatile};

pub fn inb(_addr: u16) -> u8 {
    /*unsafe {
        read_volatile(addr as *const u8)
    }*/
    0
}

pub fn inw(addr: usize) -> u16 {
    unsafe {
        read_volatile(addr as *const u16)
    }
}

pub fn inl(addr: usize) -> u32 {
    unsafe {
        read_volatile(addr as *const u32)
    }
}

pub fn inu64(addr: usize) -> u64 {
    unsafe {
        read_volatile(addr as *const u64)
    }
}

pub fn outb(addr: u16, data: u8) {
    unsafe {
        write_volatile(addr as *mut u8, data)
    }
}

pub fn outw(addr: usize, data: u16) {
    unsafe {
        write_volatile(addr as *mut u16, data)
    }
}

pub fn outl(addr: usize, data: u32) {
    unsafe {
        write_volatile(addr as *mut u32, data)
    }
}

pub fn outu64(addr: usize, data: u64) {
    unsafe {
        write_volatile(addr as *mut u64, data)
    }
}
