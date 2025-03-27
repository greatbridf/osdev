use crate::prelude::*;

use alloc::sync::Arc;
use bindings::EEXIST;
use lazy_static::lazy_static;

pub struct Console {
    terminal: Option<Arc<Terminal>>,
}

impl Console {
    pub fn get_terminal(&self) -> Option<Arc<Terminal>> {
        self.terminal.clone()
    }

    pub fn register_terminal(terminal: &Arc<Terminal>) -> KResult<()> {
        let mut console = CONSOLE.lock_irq();
        if console.terminal.is_some() {
            return Err(EEXIST);
        }

        console.terminal = Some(terminal.clone());
        Ok(())
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if let Some(console) = &self.terminal {
            for &ch in s.as_bytes() {
                console.show_char(ch)
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
    pub static ref CONSOLE: Spin<Console> = Spin::new(Console { terminal: None });
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

#[allow(unused_macros)]
macro_rules! println_info {
    ($($arg:tt)*) => {
        $crate::println!("[kernel: info] {}", format_args!($($arg)*))
    };
}

macro_rules! println_fatal {
    () => {
        $crate::println!("[kernel:fatal] ")
    };
    ($($arg:tt)*) => {
        $crate::println!("[kernel:fatal] {}", format_args!($($arg)*))
    };
}

macro_rules! println_trace {
    ($feat:literal) => {
        #[deny(unexpected_cfgs)]
        {
            #[cfg(feature = $feat)]
            $crate::println!("[kernel:trace] ")
        }
    };
    ($feat:literal, $($arg:tt)*) => {
        #[deny(unexpected_cfgs)]
        {
            #[cfg(feature = $feat)]
            $crate::println!("[kernel:trace] {}", format_args!($($arg)*))
        }
    };
}

use super::terminal::Terminal;

pub(crate) use {
    print, println, println_debug, println_fatal, println_info, println_trace, println_warn,
};
