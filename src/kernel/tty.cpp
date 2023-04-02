#include <kernel/event/evtqueue.hpp>
#include <kernel/hw/serial.h>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>

#define TTY_DATA (1 << 0)
#define TTY_EOF (1 << 1)
#define TTY_INT (1 << 2)

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
        if (this->buf.empty()) {
            while (this->blocklist.empty()) {
                current_thread->attr.ready = 0;
                current_thread->attr.wait = 1;
                this->blocklist.subscribe(current_thread);
                schedule();

                if (!this->blocklist.empty()) {
                    this->blocklist.unsubscribe(current_thread);
                    break;
                }
            }

            auto evt = this->blocklist.front();
            switch ((int)evt.data1) {
            // INTERRUPT
            case TTY_INT:
                return -1;
            // DATA
            case TTY_DATA:
                break;
            // EOF
            case TTY_EOF:
                return orig_n - n;
            }
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
        this->blocklist.push(kernel::evt { nullptr, (void*)TTY_DATA, nullptr, nullptr });
        this->blocklist.notify();
        break;
    // ^?: backspace
    case 0x7f:
        if (!buf.empty() && buf.back() != '\n') {
            buf.pop();

            if (echo) {
                serial_send_data(PORT_SERIAL0, 0x08);
                serial_send_data(PORT_SERIAL0, '\x1b');
                serial_send_data(PORT_SERIAL0, '[');
                serial_send_data(PORT_SERIAL0, 'K');
            }
        }
        break;
    // ^U: clear the line
    case 0x15:
        while (!buf.empty() && buf.back() != '\n') {
            buf.pop();

            if (echo) {
                // clear the line
                // serial_send_data(PORT_SERIAL0, '\r');
                // serial_send_data(PORT_SERIAL0, '\x1b');
                // serial_send_data(PORT_SERIAL0, '[');
                // serial_send_data(PORT_SERIAL0, '2');
                // serial_send_data(PORT_SERIAL0, 'K');
                serial_send_data(PORT_SERIAL0, 0x08);
                serial_send_data(PORT_SERIAL0, '\x1b');
                serial_send_data(PORT_SERIAL0, '[');
                serial_send_data(PORT_SERIAL0, 'K');
            }
        }
        break;
    // ^C: SIGINT
    case 0x03:
        this->blocklist.push(kernel::evt { nullptr, (void*)TTY_INT, nullptr, nullptr });
        this->blocklist.notify();
        procs->send_signal_grp(fg_pgroup, kernel::SIGINT);
        break;
    // ^D: EOF
    case 0x04:
        this->blocklist.push(kernel::evt { nullptr, (void*)TTY_EOF, nullptr, nullptr });
        this->blocklist.notify();
        break;
    // ^Z: SIGSTOP
    case 0x1a:
        this->blocklist.push(kernel::evt { nullptr, (void*)TTY_INT, nullptr, nullptr });
        this->blocklist.notify();
        procs->send_signal_grp(fg_pgroup, kernel::SIGSTOP);
        break;
    // ^\: SIGQUIT
    case 0x1c:
        this->blocklist.push(kernel::evt { nullptr, (void*)TTY_INT, nullptr, nullptr });
        this->blocklist.notify();
        procs->send_signal_grp(fg_pgroup, kernel::SIGQUIT);
        break;
    default:
        buf.put(c);
        if (echo)
            serial_send_data(PORT_SERIAL0, c);
        break;
    }
}
