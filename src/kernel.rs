pub mod block;
pub mod console;
pub mod constants;
pub mod cpu;
pub mod interrupt;
pub mod mem;
pub mod syscall;
pub mod task;
pub mod timer;
pub mod user;
pub mod vfs;

#[cfg(feature = "smp")]
pub mod smp;

mod chardev;
mod terminal;

#[allow(unused_imports)]
pub use chardev::{CharDevice, CharDeviceType, VirtualCharDevice};
pub use terminal::{Terminal, TerminalDevice};
