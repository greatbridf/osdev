#include <kernel/hw/serial.h>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>

tty::tty()
    : buf(BUFFER_SIZE)
{
}

void tty::print(const char* str)
{
    while (*str != '\0')
        this->putchar(*(str++));
}

size_t tty::read(char* buf, size_t buf_size, size_t n)
{
    size_t orig_n = n;

    while (buf_size && n) {
        while (this->buf.empty()) {
            current_thread->attr.ready = 0;
            current_thread->attr.wait = 1;
            this->blocklist.subscribe(current_thread);
            schedule();
            this->blocklist.unsubscribe(current_thread);
        }

        *buf = this->buf.get();
        --buf_size;
        --n;

        if (*(buf++) == '\n')
            break;
    }

    return orig_n - n;
}

vga_tty::vga_tty()
{
    snprintf(this->name, sizeof(this->name), "ttyVGA");
}

serial_tty::serial_tty(int id)
    : id(id)
{
    snprintf(this->name, sizeof(this->name), "ttyS%x", (int)id);
}

void serial_tty::putchar(char c)
{
    serial_send_data(id, c);
}

void vga_tty::putchar(char c)
{
    static struct vga_char vc = { .c = '\0', .color = VGA_CHAR_COLOR_WHITE };
    vc.c = c;
    vga_put_char(&vc);
}

void vga_tty::recvchar(char c)
{
    // TODO: keyboard scan code
    buf.put(c);
}

void serial_tty::recvchar(char c)
{
    switch (c) {
    case '\r':
        buf.put('\n');
        if (echo) {
            serial_send_data(PORT_SERIAL0, '\r');
            serial_send_data(PORT_SERIAL0, '\n');
        }
        this->blocklist.notify();
        break;
    // ^?: backspace
    case 0x7f:
        if (!buf.empty() && buf.back() != '\n')
            buf.pop();

        if (echo) {
            serial_send_data(PORT_SERIAL0, 0x08);
            serial_send_data(PORT_SERIAL0, '\x1b');
            serial_send_data(PORT_SERIAL0, '[');
            serial_send_data(PORT_SERIAL0, 'K');
        }
        break;
    // ^U: clear the line
    case 0x15:
        while (!buf.empty() && buf.back() != '\n')
            buf.pop();

        if (echo) {
            serial_send_data(PORT_SERIAL0, '\r');
            serial_send_data(PORT_SERIAL0, '\x1b');
            serial_send_data(PORT_SERIAL0, '[');
            serial_send_data(PORT_SERIAL0, '2');
            serial_send_data(PORT_SERIAL0, 'K');
        }
        break;
    default:
        buf.put(c);
        if (echo)
            serial_send_data(PORT_SERIAL0, c);
        break;
    }
}
