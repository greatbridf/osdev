#include <errno.h>
#include <stdio.h>

#include <asm/port_io.h>
#include <kernel/hw/serial.h>
#include <kernel/irq.hpp>
#include <kernel/tty.hpp>

static void serial_receive_data_interrupt(void)
{
    while (is_serial_has_data(PORT_SERIAL0)) {
        uint8_t data = serial_read_data(PORT_SERIAL0);
        console->commit_char(data);
    }
}

SECTION(".text.kinit")
int32_t init_serial_port(port_id_t port)
{
    // taken from osdev.org

    asm_outb(port + 1, 0x00); // Disable all interrupts
    asm_outb(port + 3, 0x80); // Enable DLAB (set baud rate divisor)
    // TODO: set baud rate
    asm_outb(port + 0, 0x00); // Set divisor to 0 -3- (lo byte) 115200 -38400- baud
    asm_outb(port + 1, 0x00); //                  (hi byte)
    asm_outb(port + 3, 0x03); // 8 bits, no parity, one stop bit
    asm_outb(port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
    // TODO: IRQ disabled
    asm_outb(port + 4, 0x0B); // IRQs enabled, RTS/DSR set
    asm_outb(port + 4, 0x1E); // Set in loopback mode, test the serial chip
    asm_outb(port + 0, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)

    // Check if serial is faulty (i.e: not same byte as sent)
    if (asm_inb(port + 0) != 0xAE)
        return -EIO;

    // If serial is not faulty set it in normal operation mode
    // (not-loopback with IRQs enabled and OUT#1 and OUT#2 bits enabled)
    asm_outb(port + 4, 0x0F);

    asm_outb(port + 1, 0x01); // Enable interrupts #0: Received Data Available

    kernel::irq::register_handler(4, serial_receive_data_interrupt);

    return 0;
}

int32_t is_serial_has_data(port_id_t port)
{
    return asm_inb(port + 5) & 1;
}

uint8_t serial_read_data(port_id_t port)
{
    while (is_serial_has_data(port) == 0)
        ;
    return asm_inb(port);
}

int32_t is_serial_ready_for_transmition(port_id_t port)
{
    return asm_inb(port + 5) & 0x20;
}

void serial_send_data(port_id_t port, uint8_t data)
{
    while (is_serial_ready_for_transmition(port) == 0)
        ;
    return asm_outb(port, data);
}
