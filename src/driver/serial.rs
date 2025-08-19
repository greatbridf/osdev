mod io;

use crate::{
    kernel::{
        block::make_device, console::set_console, constants::EIO, interrupt::register_irq_handler,
        CharDevice, CharDeviceType, Terminal, TerminalDevice,
    },
    prelude::*,
};
use alloc::{collections::vec_deque::VecDeque, format, sync::Arc};
use bitflags::bitflags;
use core::pin::pin;
use eonix_runtime::scheduler::RUNTIME;
use eonix_sync::{SpinIrq as _, WaitList};
use io::SerialIO;

bitflags! {
    struct LineStatus: u8 {
        const RX_READY = 0x01;
        const TX_READY = 0x20;
    }
}

trait SerialRegister {
    fn read(&self) -> u8;
    fn write(&self, value: u8);
}

#[allow(dead_code)]
struct Serial {
    id: u32,
    name: Arc<str>,

    terminal: Spin<Option<Arc<Terminal>>>,
    worker_wait: WaitList,

    working: Spin<bool>,
    tx_buffer: Spin<VecDeque<u8>>,

    ioregs: SerialIO,
}

impl Serial {
    fn enable_interrupts(&self) {
        // Enable interrupt #0: Received data available
        self.ioregs.int_ena().write(0x03);
    }

    fn disable_interrupts(&self) {
        // Disable interrupt #0: Received data available
        self.ioregs.int_ena().write(0x02);
    }

    fn line_status(&self) -> LineStatus {
        LineStatus::from_bits_truncate(self.ioregs.line_status().read())
    }

    async fn wait_for_interrupt(&self) {
        let mut wait = pin!(self.worker_wait.prepare_to_wait());

        {
            let mut working = self.working.lock_irq();
            self.enable_interrupts();
            wait.as_mut().add_to_wait_list();
            *working = false;
        };

        wait.await;

        *self.working.lock_irq() = true;
        self.disable_interrupts();
    }

    async fn worker(port: Arc<Self>) {
        let terminal = port.terminal.lock().clone();

        loop {
            while port.line_status().contains(LineStatus::RX_READY) {
                let ch = port.ioregs.tx_rx().read();

                if let Some(terminal) = terminal.as_ref() {
                    terminal.commit_char(ch).await;
                }
            }

            let should_wait = {
                let mut tx_buffer = port.tx_buffer.lock();
                let mut count = 0;

                // Give it a chance to receive data.
                for &ch in tx_buffer.iter().take(64) {
                    if port.line_status().contains(LineStatus::TX_READY) {
                        port.ioregs.tx_rx().write(ch);
                    } else {
                        break;
                    }
                    count += 1;
                }
                tx_buffer.drain(..count);

                tx_buffer.is_empty()
            };

            if should_wait {
                port.wait_for_interrupt().await;
            }
        }
    }

    pub fn new(id: u32, ioregs: SerialIO) -> KResult<Self> {
        ioregs.int_ena().write(0x00); // Disable all interrupts
        ioregs.line_control().write(0x80); // Enable DLAB (set baud rate divisor)
        ioregs.tx_rx().write(0x00); // Set divisor to 0 (lo byte) 115200 baud rate
        ioregs.int_ena().write(0x00); //              0 (hi byte)
        ioregs.line_control().write(0x03); // 8 bits, no parity, one stop bit
        ioregs.int_ident().write(0xc7); // Enable FIFO, clear them, with 14-byte threshold
        ioregs.modem_control().write(0x0b); // IRQs enabled, RTS/DSR set
        ioregs.modem_control().write(0x1e); // Set in loopback mode, test the serial chip
        ioregs.tx_rx().write(0x19); // Test serial chip (send byte 0x19 and check if serial returns
                                    // same byte)
        if ioregs.tx_rx().read() != 0x19 {
            return Err(EIO);
        }

        ioregs.modem_control().write(0x0f); // Return to normal operation mode

        Ok(Self {
            id,
            name: Arc::from(format!("ttyS{id}")),
            terminal: Spin::new(None),
            worker_wait: WaitList::new(),
            working: Spin::new(true),
            tx_buffer: Spin::new(VecDeque::new()),
            ioregs,
        })
    }

    fn wakeup_worker(&self) {
        let working = self.working.lock_irq();
        if !*working {
            self.worker_wait.notify_one();
        }
    }

    fn irq_handler(&self) {
        // Read the interrupt ID register to clear the interrupt.
        self.ioregs.int_ident().read();
        self.wakeup_worker();
    }

    fn register_as_char_device(self, irq_no: usize) -> KResult<()> {
        let port = Arc::new(self);
        let terminal = Terminal::new(port.clone());

        port.terminal.lock().replace(terminal.clone());

        {
            let port = port.clone();
            register_irq_handler(irq_no as i32, move || {
                port.irq_handler();
            })?;
        }

        RUNTIME.spawn(Self::worker(port.clone()));

        let _ = set_console(terminal.clone());
        eonix_log::set_console(terminal.clone());

        CharDevice::register(
            make_device(4, 64 + port.id),
            port.name.clone(),
            CharDeviceType::Terminal(terminal),
        )?;

        Ok(())
    }
}

impl TerminalDevice for Serial {
    fn write(&self, data: &[u8]) {
        let mut tx_buffer = self.tx_buffer.lock();
        tx_buffer.extend(data.iter().copied());
        self.wakeup_worker();
    }

    fn write_direct(&self, data: &[u8]) {
        for &ch in data {
            self.ioregs.tx_rx().write(ch);
        }
    }
}

pub fn init() -> KResult<()> {
    #[cfg(target_arch = "x86_64")]
    {
        let (com0, com1) = unsafe {
            const COM0_BASE: u16 = 0x3f8;
            const COM1_BASE: u16 = 0x2f8;
            // SAFETY: The COM ports are well-known hardware addresses.
            (SerialIO::new(COM0_BASE), SerialIO::new(COM1_BASE))
        };

        if let Ok(port) = Serial::new(0, com0) {
            const COM0_IRQ: usize = 4;
            port.register_as_char_device(COM0_IRQ)?;
        }

        if let Ok(port) = Serial::new(1, com1) {
            const COM1_IRQ: usize = 3;
            port.register_as_char_device(COM1_IRQ)?;
        }
    }

    #[cfg(target_arch = "riscv64")]
    {
        use eonix_hal::platform::UART_BASE;
        use eonix_hal::platform::UART_IRQ;

        let port = unsafe {
            // SAFETY: The base address is provided by the FDT and should be valid.
            SerialIO::new(UART_BASE)
        };

        let serial = Serial::new(0, port)?;
        serial.register_as_char_device(UART_IRQ)?;
    }

    #[cfg(target_arch = "loongarch64")]
    {
        use eonix_mm::address::PAddr;

        let port = unsafe {
            // SAFETY: The base address is provided by the FDT and should be valid.
            SerialIO::new(PAddr::from(0x1fe0_01e0))
        };

        let serial = Serial::new(0, port)?;
        serial.register_as_char_device(
            // 2 or 4 here, let's try 2 first!
            2,
        )?;
    }

    Ok(())
}
