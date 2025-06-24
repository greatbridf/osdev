use crate::prelude::*;
use alloc::sync::Arc;

static CONSOLE: Spin<Option<Arc<Terminal>>> = Spin::new(None);

pub fn set_console(terminal: Arc<Terminal>) -> KResult<()> {
    let mut console = CONSOLE.lock();
    if console.is_none() {
        *console = Some(terminal);
        Ok(())
    } else {
        Err(EEXIST)
    }
}

pub fn get_console() -> Option<Arc<Terminal>> {
    let console = CONSOLE.lock();
    console.clone()
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    // TODO!!!!!!!!!!!!!: REMOVE THIS AND USE `eonix_log`.
    eonix_log::do_print(args);
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

#[allow(unused_macros)]
macro_rules! println_trace {
    ($feat:literal) => {
        #[deny(unexpected_cfgs)]
        {
            #[cfg(feature = $feat)]
            $crate::println!("[kernel:trace] ")
        }
    };
    ($feat:literal, $($arg:tt)*) => {{
        #[deny(unexpected_cfgs)]
        {
            #[cfg(feature = $feat)]
            $crate::println!("[kernel:trace] {}", format_args!($($arg)*))
        }
    }};
}

use super::{constants::EEXIST, terminal::Terminal};

pub(crate) use {
    print, println, println_debug, println_fatal, println_info, println_trace, println_warn,
};
