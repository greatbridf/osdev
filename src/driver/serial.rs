use alloc::{format, sync::Arc};
use bindings::EIO;

use crate::{
    kernel::{
        block::make_device, interrupt::register_irq_handler, CharDevice, CharDeviceType, Console,
        Terminal, TerminalDevice,
    },
    prelude::*,
};

use super::Port8;

struct Serial {
    id: u32,
    name: Arc<str>,

    terminal: Option<Arc<Terminal>>,

    tx_rx: Port8,
    int_ena: Port8,
    int_ident: Port8,
    line_control: Port8,
    modem_control: Port8,
    line_status: Port8,
    modem_status: Port8,
    scratch: Port8,
}

impl Serial {
    const COM0_BASE: u16 = 0x3f8;
    const COM1_BASE: u16 = 0x2f8;

    const COM0_IRQ: u8 = 4;
    const COM1_IRQ: u8 = 3;

    fn enable_interrupts(&self) {
        // Enable interrupt #0: Received data available
        self.int_ena.write(0x01);
    }

    pub fn new(id: u32, base_port: u16) -> KResult<Self> {
        let port = Self {
            id,
            name: Arc::from(format!("ttyS{id}")),
            terminal: None,
            tx_rx: Port8::new(base_port),
            int_ena: Port8::new(base_port + 1),
            int_ident: Port8::new(base_port + 2),
            line_control: Port8::new(base_port + 3),
            modem_control: Port8::new(base_port + 4),
            line_status: Port8::new(base_port + 5),
            modem_status: Port8::new(base_port + 6),
            scratch: Port8::new(base_port + 7),
        };

        port.int_ena.write(0x00); // Disable all interrupts
        port.line_control.write(0x80); // Enable DLAB (set baud rate divisor)
        port.tx_rx.write(0x00); // Set divisor to 0 (lo byte) 115200 baud rate
        port.int_ena.write(0x00); //              0 (hi byte)
        port.line_control.write(0x03); // 8 bits, no parity, one stop bit
        port.int_ident.write(0xc7); // Enable FIFO, clear them, with 14-byte threshold
        port.modem_control.write(0x0b); // IRQs enabled, RTS/DSR set
        port.modem_control.write(0x1e); // Set in loopback mode, test the serial chip
        port.tx_rx.write(0x19); // Test serial chip (send byte 0x19 and check if serial returns
                                // same byte)
        if port.tx_rx.read() != 0x19 {
            return Err(EIO);
        }

        port.modem_control.write(0x0f); // Return to normal operation mode
        Ok(port)
    }

    fn irq_handler(&self) {
        let terminal = self.terminal.as_ref();
        while self.line_status.read() & 0x01 != 0 {
            let ch = self.tx_rx.read();

            if let Some(terminal) = terminal {
                terminal.commit_char(ch);
            }
        }
    }

    fn register_char_device(port: Self) -> KResult<()> {
        let mut port = Arc::new(port);
        let terminal = Terminal::new(port.clone());

        // TODO!!!!!!: This is unsafe, we should find a way to avoid this.
        //             Under smp, we should make the publish of terminal atomic.
        unsafe { Arc::get_mut_unchecked(&mut port) }.terminal = Some(terminal.clone());

        {
            let port = port.clone();
            let irq_no = match port.id {
                0 => Serial::COM0_IRQ,
                1 => Serial::COM1_IRQ,
                _ => unreachable!(),
            };

            register_irq_handler(irq_no as i32, move || {
                port.irq_handler();
            })?;
        }
        port.enable_interrupts();
        dont_check!(Console::register_terminal(&terminal));

        CharDevice::register(
            make_device(4, 64 + port.id),
            port.name.clone(),
            CharDeviceType::Terminal(terminal),
        )?;

        Ok(())
    }
}

impl TerminalDevice for Serial {
    fn putchar(&self, ch: u8) {
        loop {
            // If we poll the status and get the corresponding bit, we should handle the action.
            let status = self.line_status.read();

            // We should receive a byte and commit that to the line.
            if status & 0x01 != 0 {
                let ch = self.tx_rx.read();

                if let Some(terminal) = self.terminal.as_ref() {
                    terminal.commit_char(ch);
                }
            }

            if status & 0x20 != 0 {
                self.tx_rx.write(ch);
                return;
            }
        }
    }
}

pub fn init() -> KResult<()> {
    let com0 = Serial::new(0, Serial::COM0_BASE);
    let com1 = Serial::new(1, Serial::COM1_BASE);

    if let Ok(port) = com0 {
        Serial::register_char_device(port)?;
    }

    if let Ok(port) = com1 {
        Serial::register_char_device(port)?;
    }

    Ok(())
}
