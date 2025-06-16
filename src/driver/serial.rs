use crate::{
    kernel::{
        block::make_device, console::set_console, constants::EIO, interrupt::register_irq_handler,
        task::KernelStack, CharDevice, CharDeviceType, Terminal, TerminalDevice,
    },
    prelude::*,
};
use alloc::{collections::vec_deque::VecDeque, format, sync::Arc};
use bitflags::bitflags;
use core::pin::pin;
use eonix_hal::arch_exported::io::Port8;
use eonix_runtime::{run::FutureRun, scheduler::Scheduler};
use eonix_sync::{SpinIrq as _, WaitList};

#[cfg(not(target_arch = "x86_64"))]
compile_error!("Serial driver is only supported on x86_64 architecture");

bitflags! {
    struct LineStatus: u8 {
        const RX_READY = 0x01;
        const TX_READY = 0x20;
    }
}

#[allow(dead_code)]
struct Serial {
    id: u32,
    name: Arc<str>,

    terminal: Spin<Option<Arc<Terminal>>>,
    worker_wait: WaitList,

    working: Spin<bool>,
    tx_buffer: Spin<VecDeque<u8>>,

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
        self.int_ena.write(0x03);
    }

    fn disable_interrupts(&self) {
        // Disable interrupt #0: Received data available
        self.int_ena.write(0x02);
    }

    fn line_status(&self) -> LineStatus {
        LineStatus::from_bits_truncate(self.line_status.read())
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
                let ch = port.tx_rx.read();

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
                        port.tx_rx.write(ch);
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

    pub fn new(id: u32, base_port: u16) -> KResult<Self> {
        let port = Self {
            id,
            name: Arc::from(format!("ttyS{id}")),
            terminal: Spin::new(None),
            worker_wait: WaitList::new(),
            working: Spin::new(true),
            tx_buffer: Spin::new(VecDeque::new()),
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

    fn wakeup_worker(&self) {
        let working = self.working.lock_irq();
        if !*working {
            self.worker_wait.notify_one();
        }
    }

    fn irq_handler(&self) {
        // Read the interrupt ID register to clear the interrupt.
        self.int_ident.read();
        self.wakeup_worker();
    }

    fn register_char_device(port: Self) -> KResult<()> {
        let port = Arc::new(port);
        let terminal = Terminal::new(port.clone());

        port.terminal.lock().replace(terminal.clone());

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

        Scheduler::get().spawn::<KernelStack, _>(FutureRun::new(Self::worker(port.clone())));

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
    fn putchar(&self, ch: u8) {
        let mut tx_buffer = self.tx_buffer.lock();
        tx_buffer.push_back(ch);
        self.wakeup_worker();
    }

    fn putchar_direct(&self, ch: u8) {
        self.tx_rx.write(ch);
    }
}

pub fn init() -> KResult<()> {
    if let Ok(port) = Serial::new(0, Serial::COM0_BASE) {
        Serial::register_char_device(port)?;
    }

    if let Ok(port) = Serial::new(1, Serial::COM1_BASE) {
        Serial::register_char_device(port)?;
    }

    Ok(())
}
