use crate::prelude::*;

pub struct Console {}

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
pub fn _print(args: core::fmt::Arguments) -> core::fmt::Result {
    CONSOLE.lock().write_fmt(args)
}

pub static CONSOLE: spin::Mutex<Console> = spin::Mutex::new(Console {});

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

pub(crate) use {print, println};
