#include <kernel/event/evtqueue.hpp>
#include <kernel/hw/serial.h>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/lock.hpp>

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
        auto& mtx = this->m_cv.mtx();
        types::lock_guard lck(mtx);

        if (this->buf.empty()) {
            bool intr = !this->m_cv.wait(mtx);

            if (intr || this->buf.empty())
                break;
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
            serial_send_data(id, '\r');
            serial_send_data(id, '\n');
        }
        this->m_cv.notify();
        break;
    // ^?: backspace
    case 0x7f:
        if (!buf.empty() && buf.back() != '\n') {
            buf.pop();

            if (echo) {
                serial_send_data(id, 0x08);
                serial_send_data(id, '\x1b');
                serial_send_data(id, '[');
                serial_send_data(id, 'K');
            }
        }
        break;
    // ^U: clear the line
    case 0x15:
        while (!buf.empty() && buf.back() != '\n') {
            buf.pop();

            if (echo) {
                // clear the line
                // serial_send_data(id, '\r');
                // serial_send_data(id, '\x1b');
                // serial_send_data(id, '[');
                // serial_send_data(id, '2');
                // serial_send_data(id, 'K');
                serial_send_data(id, 0x08);
                serial_send_data(id, '\x1b');
                serial_send_data(id, '[');
                serial_send_data(id, 'K');
            }
        }
        break;
    // ^C: SIGINT
    case 0x03:
        procs->send_signal_grp(fg_pgroup, kernel::SIGINT);
        this->m_cv.notify();
        break;
    // ^D: EOF
    case 0x04:
        this->m_cv.notify();
        break;
    // ^Z: SIGSTOP
    case 0x1a:
        procs->send_signal_grp(fg_pgroup, kernel::SIGSTOP);
        this->m_cv.notify();
        break;
    // ^[: ESCAPE
    case 0x1b:
        buf.put('\x1b');
        if (echo) {
            serial_send_data(id, '^');
            serial_send_data(id, '[');
        }
        break;
    // ^\: SIGQUIT
    case 0x1c:
        procs->send_signal_grp(fg_pgroup, kernel::SIGQUIT);
        this->m_cv.notify();
        break;
    default:
        buf.put(c);
        if (echo)
            serial_send_data(id, c);
        break;
    }
}
