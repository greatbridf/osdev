#include <errno.h>
#include <stdio.h>

#include <kernel/hw/port.hpp>
#include <kernel/irq.hpp>
#include <kernel/module.hpp>
#include <kernel/tty.hpp>

constexpr int PORT0 = 0x3f8;
constexpr int PORT1 = 0x2f8;

using port_group = kernel::hw::p8[6];

constexpr kernel::hw::p8 port0[] = {
    kernel::hw::p8{PORT0+0},
    kernel::hw::p8{PORT0+1},
    kernel::hw::p8{PORT0+2},
    kernel::hw::p8{PORT0+3},
    kernel::hw::p8{PORT0+4},
    kernel::hw::p8{PORT0+5},
};

constexpr kernel::hw::p8 port1[] = {
    kernel::hw::p8{PORT1+0},
    kernel::hw::p8{PORT1+1},
    kernel::hw::p8{PORT1+2},
    kernel::hw::p8{PORT1+3},
    kernel::hw::p8{PORT1+4},
    kernel::hw::p8{PORT1+5},
};

static inline bool _serial_has_data(port_group ports)
{
    return *ports[5] & 1;
}

static inline int _init_port(port_group ports)
{
    // taken from osdev.org

    ports[1] = 0x00; // Disable all interrupts
    ports[3] = 0x80; // Enable DLAB (set baud rate divisor)
    // TODO: set baud rate
    ports[0] = 0x00; // Set divisor to 0 -3- (lo byte) 115200 -38400- baud
    ports[1] = 0x00; //                  (hi byte)
    ports[3] = 0x03; // 8 bits, no parity, one stop bit
    ports[2] = 0xC7; // Enable FIFO, clear them, with 14-byte threshold
    // TODO: IRQ disabled
    ports[4] = 0x0B; // IRQs enabled, RTS/DSR set
    ports[4] = 0x1E; // Set in loopback mode, test the serial chip
    ports[0] = 0xAE; // Test serial chip (send byte 0xAE and check if serial returns same byte)

    // Check if serial is faulty (i.e: not same byte as sent)
    if (*ports[0] != 0xAE)
        return -EIO;

    // If serial is not faulty set it in normal operation mode
    // (not-loopback with IRQs enabled and OUT#1 and OUT#2 bits enabled)
    ports[4] = 0x0F;

    ports[1] = 0x01; // Enable interrupts #0: Received Data Available

    // TODO: LONG MODE
    // kernel::irq::register_handler(4, serial_receive_data_interrupt);

    return 0;
}

// TODO: LONG MODE
// static void serial_receive_data_interrupt(void)
// {
//     while (_serial_has_data(PORT_SERIAL0)) {
//         uint8_t data = serial_read_data(PORT_SERIAL0);
//         console->commit_char(data);
//     }
// }
// 
// uint8_t serial_read_data(port_id_t port)
// {
//     while (is_serial_has_data(port) == 0)
//         ;
//     return asm_inb(port);
// }
// 
// int32_t is_serial_ready_for_transmition(port_id_t port)
// {
//     return asm_inb(port + 5) & 0x20;
// }
// 
// void serial_send_data(port_id_t port, uint8_t data)
// {
//     while (is_serial_ready_for_transmition(port) == 0)
//         ;
//     return asm_outb(port, data);
// }

class serial_tty : public virtual tty {
public:
    serial_tty(int id): id(id)
    {
        snprintf(name, sizeof(name), "ttyS%d", id);
    }

    virtual void putchar(char c) override
    {
        // TODO: LONG MODE
        // serial_send_data(id, c);
    }

public:
    uint16_t id;
};

class serial_module : public virtual kernel::module::module {
public:
    serial_module() : module("serial-tty") { }

    virtual int init() override
    {
        // TODO: LONG MODE
        return kernel::module::MODULE_FAILED;
    }

};

kernel::module::module* serial_module_init()
{ return new serial_module(); }
INTERNAL_MODULE(serial_module_loader, serial_module_init);
