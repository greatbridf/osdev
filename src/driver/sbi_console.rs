use crate::kernel::{block::make_device, CharDevice, CharDeviceType, Terminal, TerminalDevice};
use alloc::sync::Arc;
use eonix_log::ConsoleWrite;

struct SbiConsole;

impl ConsoleWrite for SbiConsole {
    fn write(&self, s: &str) {
        eonix_hal::bootstrap::early_console_write(s);
    }
}

impl TerminalDevice for SbiConsole {
    fn write(&self, data: &[u8]) {
        for &ch in data {
            eonix_hal::bootstrap::early_console_putchar(ch);
        }
    }

    fn write_direct(&self, data: &[u8]) {
        for &ch in data {
            eonix_hal::bootstrap::early_console_putchar(ch);
        }
    }
}

pub fn init_console() {
    eonix_log::set_console(Arc::new(SbiConsole));

    let console = Arc::new(SbiConsole);
    let terminal = Terminal::new(console.clone());
    crate::kernel::console::set_console(terminal.clone()).expect("Failed to set console");

    CharDevice::register(
        make_device(4, 64),
        Arc::from("sbi_console"),
        CharDeviceType::Terminal(terminal),
    )
    .expect("Failed to register SBI console as a character device");
}
