use crate::prelude::*;

use lazy_static::lazy_static;

pub struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        use crate::bindings::root::kernel::tty::console as _console;

        if let Some(console) = unsafe { _console.as_mut() } {
            for &ch in s.as_bytes() {
                unsafe {
                    console.show_char(ch as i32);
                }
            }
        }

        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    dont_check!(CONSOLE.lock_irq().write_fmt(args))
}

lazy_static! {
    pub static ref CONSOLE: Spin<Console> = Spin::new(Console {});
}

macro_rules! print {
    ($($arg:tt)*) => {
        $crate::kernel::console::_print(format_args!($($arg)*))
    };
}

macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

macro_rules! println_warn {
    ($($arg:tt)*) => {
        $crate::println!("[kernel: warn] {}", format_args!($($arg)*))
    };
}

macro_rules! println_debug {
    ($($arg:tt)*) => {
        $crate::println!("[kernel:debug] {}", format_args!($($arg)*))
    };
}

macro_rules! println_info {
    ($($arg:tt)*) => {
        $crate::println!("[kernel: info] {}", format_args!($($arg)*))
    };
}

macro_rules! println_fatal {
    ($($arg:tt)*) => {
        $crate::println!("[kernel:fatal] {}", format_args!($($arg)*))
    };
}

pub(crate) use {
    print, println, println_debug, println_fatal, println_info, println_warn,
};
