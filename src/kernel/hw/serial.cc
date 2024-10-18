#include <errno.h>
#include <stdio.h>

#include <kernel/hw/port.hpp>
#include <kernel/irq.hpp>
#include <kernel/log.hpp>
#include <kernel/module.hpp>
#include <kernel/tty.hpp>

using namespace kernel::tty;
using namespace kernel::hw;
using namespace kernel::irq;
using namespace kernel::kmod;

constexpr int PORT0 = 0x3f8;
constexpr int PORT1 = 0x2f8;

using port_group = const p8[6];

constexpr p8 port0[] = {
    p8{PORT0 + 0}, p8{PORT0 + 1}, p8{PORT0 + 2},
    p8{PORT0 + 3}, p8{PORT0 + 4}, p8{PORT0 + 5},
};

constexpr p8 port1[] = {
    p8{PORT1 + 0}, p8{PORT1 + 1}, p8{PORT1 + 2},
    p8{PORT1 + 3}, p8{PORT1 + 4}, p8{PORT1 + 5},
};

static void _serial0_receive_data_interrupt() {
    while (*port0[5] & 1)
        console->commit_char(*port0[0]);
}

static void _serial1_receive_data_interrupt() {
    while (*port1[5] & 1)
        console->commit_char(*port1[0]);
}

static inline int _init_port(port_group ports) {
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
    ports[0] = 0xAE; // Test serial chip (send byte 0xAE and check if serial
                     // returns same byte)

    // Check if serial is faulty (i.e: not same byte as sent)
    if (*ports[0] != 0xAE)
        return -EIO;

    // If serial is not faulty set it in normal operation mode
    // (not-loopback with IRQs enabled and OUT#1 and OUT#2 bits enabled)
    ports[4] = 0x0F;

    ports[1] = 0x01; // Enable interrupts #0: Received Data Available

    return 0;
}

class serial_tty : public virtual tty {
    const p8* ports;

   public:
    serial_tty(port_group ports, int id) : tty{"ttyS"}, ports(ports) {
        name += '0' + id;
    }

    virtual void putchar(char c) override {
        while (true) {
            auto status = *ports[5];
            if (status & 0x1)
                this->commit_char(*ports[0]);
            if (status & 0x20)
                break;
        }

        ports[0] = c;
    }
};

class serial_module : public virtual kmod {
   public:
    serial_module() : kmod("serial-tty") {}

    virtual int init() override {
        if (int ret = _init_port(port0); ret == 0) {
            auto* dev = new serial_tty(port0, 0);
            register_handler(4, _serial0_receive_data_interrupt);

            if (int ret = register_tty(dev); ret != 0)
                kmsg("[serial] cannot register ttyS0");
        }

        if (int ret = _init_port(port1); ret == 0) {
            auto* dev = new serial_tty(port1, 0);
            register_handler(3, _serial1_receive_data_interrupt);

            if (int ret = register_tty(dev); ret != 0)
                kmsg("[serial] cannot register ttyS1");
        }

        return 0;
    }
};

INTERNAL_MODULE(serial, serial_module);
