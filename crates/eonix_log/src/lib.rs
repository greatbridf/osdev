#![no_std]

use core::fmt::{self, Write};

use alloc::sync::Arc;
use eonix_spin_irq::SpinIrq as _;
use eonix_sync::Spin;

extern crate alloc;

pub trait ConsoleWrite: Send + Sync {
    fn write(&self, s: &str);
}

struct Console {
    console: Option<Arc<dyn ConsoleWrite>>,
}

// TODO!!!: We should use a `RwLock` here for better performance.
static CONSOLE: Spin<Console> = Spin::new(Console::new());

impl Console {
    const fn new() -> Self {
        Self { console: None }
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(console) = self.console.as_ref() {
            console.write(s);
        }
        Ok(())
    }
}

pub fn set_console(console: Arc<dyn ConsoleWrite>) {
    CONSOLE.lock_irq().console.replace(console);
}

#[doc(hidden)]
pub fn do_print(args: fmt::Arguments) {
    let _ = CONSOLE.lock_irq().write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::do_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println_warn {
    ($($arg:tt)*) => {
        $crate::println!("[kernel: warn] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println_debug {
    ($($arg:tt)*) => {
        $crate::println!("[kernel:debug] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println_info {
    ($($arg:tt)*) => {
        $crate::println!("[kernel: info] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println_fatal {
    () => {
        $crate::println!("[kernel:fatal] ")
    };
    ($($arg:tt)*) => {
        $crate::println!("[kernel:fatal] {}", format_args!($($arg)*))
    };
}

#[macro_export]
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
