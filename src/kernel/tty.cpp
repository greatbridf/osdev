#include <algorithm>

#include <stdint.h>
#include <stdio.h>
#include <termios.h>

#include <kernel/async/lock.hpp>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <kernel/log.hpp>

#define CTRL(key) ((key)-0x40)

#define TERMIOS_ISET(termios, option) ((option) == ((termios).c_iflag & (option)))
#define TERMIOS_OSET(termios, option) ((option) == ((termios).c_oflag & (option)))
#define TERMIOS_CSET(termios, option) ((option) == ((termios).c_cflag & (option)))
#define TERMIOS_LSET(termios, option) ((option) == ((termios).c_lflag & (option)))

#define TERMIOS_TESTCC(c, termios, cc) ((c != 0xff) && (c == ((termios).c_cc[cc])))

using namespace kernel::tty;

tty::tty(std::string name)
    : termio {
        .c_iflag = ICRNL | IXOFF,
        .c_oflag = OPOST | ONLCR,
        .c_cflag = B38400 | CS8 | CREAD | HUPCL,
        .c_lflag = ISIG | ICANON | ECHO | ECHOE |
            ECHOK | ECHOCTL | ECHOKE | IEXTEN,
        .c_line = N_TTY,
        .c_cc {},
        .c_ispeed = 38400,
        .c_ospeed = 38400,
    }
    , name{name}
    , buf(BUFFER_SIZE)
    , fg_pgroup { 0 }
{
    memset(this->termio.c_cc, 0x00, sizeof(this->termio.c_cc));

    // other special characters is not supported for now
    this->termio.c_cc[VINTR] = CTRL('C');
    this->termio.c_cc[VQUIT] = CTRL('\\');
    this->termio.c_cc[VERASE] = 0x7f;
    this->termio.c_cc[VKILL] = CTRL('U');
    this->termio.c_cc[VEOF] = CTRL('D');
    this->termio.c_cc[VSUSP] = CTRL('Z');
    this->termio.c_cc[VMIN] = 1;
}

void tty::print(const char* str)
{
    while (*str != '\0')
        this->putchar(*(str++));
}

int tty::poll()
{
    async::lock_guard_irq lck(this->mtx_buf);
    if (this->buf.empty()) {
        bool interrupted = this->waitlist.wait(this->mtx_buf);

        if (interrupted)
            return -EINTR;
    }

    return 1;
}

ssize_t tty::read(char* buf, size_t buf_size, size_t n)
{
    n = std::max(buf_size, n);
    size_t orig_n = n;

    do {
        if (n == 0)
            break;

        async::lock_guard_irq lck(this->mtx_buf);

        if (this->buf.empty()) {
            bool interrupted = this->waitlist.wait(this->mtx_buf);

            if (interrupted)
                break;
        }

        if (this->buf.empty())
            break;

        if (!TERMIOS_LSET(this->termio, ICANON)) {
            --n, *buf = this->buf.get();
            break;
        }

        while (n &&!this->buf.empty()) {
            int c = this->buf.get();

            --n, *(buf++) = c;

            // canonical mode
            if (c == '\n')
                break;
        }
    } while (false);

    return orig_n - n;
}

int tty::_do_erase(bool should_echo)
{
    if (buf.empty())
        return -1;

    int back = buf.back();

    if (back == '\n' || back == this->termio.c_cc[VEOF])
        return -1;

    if (back == this->termio.c_cc[VEOL] || back == this->termio.c_cc[VEOL2])
        return -1;

    buf.pop();

    if (should_echo && TERMIOS_LSET(this->termio, ECHO | ECHOE)) {
        this->show_char('\b'); // backspace
        this->show_char(' '); // space
        this->show_char('\b'); // backspace

        // xterm's way to show backspace
        // serial_send_data(id, '\b');
        // serial_send_data(id, CTRL('['));
        // serial_send_data(id, '[');
        // serial_send_data(id, 'K');
    }

    return back;
}

void tty::_real_commit_char(int c)
{
    switch (c) {
    case '\n':
        buf.put(c);

        if (TERMIOS_LSET(this->termio, ECHONL) || TERMIOS_LSET(this->termio, ECHO))
            this->_echo_char(c);

        // if ICANON is set, we notify all waiting processes
        // if ICANON is not set, since there are data ready, we notify as well
        this->waitlist.notify_all();

        break;

    default:
        buf.put(c);

        if (TERMIOS_LSET(this->termio, ECHO))
            this->_echo_char(c);

        if (!TERMIOS_LSET(this->termio, ICANON))
            this->waitlist.notify_all();

        break;
    }
}

void tty::_echo_char(int c)
{
    // ECHOCTL
    do {
        if (c < 0 || c >= 32 || !TERMIOS_LSET(this->termio, ECHO | ECHOCTL | IEXTEN))
            break;

        if (c == '\t' || c == '\n' || c == CTRL('Q') || c == CTRL('S'))
            break;

        this->show_char('^');
        this->show_char(c + 0x40);

        return;
    } while (false);

    this->show_char(c);
}

// do some ignore and remapping work
// real commit operation is in _real_commit_char()
void tty::commit_char(int c)
{
    // check special control characters
    // if handled, the character is discarded
    if (TERMIOS_LSET(this->termio, ISIG)) {
        if (TERMIOS_TESTCC(c, this->termio, VINTR)) {
            if (!TERMIOS_LSET(this->termio, NOFLSH))
                this->clear_read_buf();

            this->_echo_char(c);
            procs->send_signal_grp(fg_pgroup, SIGINT);

            return;
        }

        if (TERMIOS_TESTCC(c, this->termio, VSUSP)) {
            if (!TERMIOS_LSET(this->termio, NOFLSH))
                this->clear_read_buf();

            this->_echo_char(c);
            procs->send_signal_grp(fg_pgroup, SIGTSTP);

            return;
        }

        if (TERMIOS_TESTCC(c, this->termio, VQUIT)) {
            if (!TERMIOS_LSET(this->termio, NOFLSH))
                this->clear_read_buf();

            this->_echo_char(c);
            procs->send_signal_grp(fg_pgroup, SIGQUIT);

            return;
        }
    }

    // if handled, the character is discarded
    if (TERMIOS_LSET(this->termio, ICANON)) {
        if (TERMIOS_TESTCC(c, this->termio, VEOF)) {
            this->waitlist.notify_all();
            return;
        }

        if (TERMIOS_TESTCC(c, this->termio, VKILL)) {
            if (TERMIOS_LSET(this->termio, ECHOKE | IEXTEN)) {
                while (this->_do_erase(true) != -1)
                    ;
            }
            else if (TERMIOS_LSET(this->termio, ECHOK)) {
                while (this->_do_erase(false) != -1)
                    ;
                this->show_char('\n');
            }
            return;
        }

        if (TERMIOS_TESTCC(c, this->termio, VERASE)) {
            this->_do_erase(true);
            return;
        }
    }

    switch (c) {
    case '\r':
        if (TERMIOS_ISET(this->termio, IGNCR))
            break;

        if (TERMIOS_ISET(this->termio, ICRNL)) {
            this->_real_commit_char('\n');
            break;
        }

        this->_real_commit_char('\r');
        break;

    case '\n':
        if (TERMIOS_ISET(this->termio, INLCR)) {
            this->_real_commit_char('\r');
            break;
        }

        this->_real_commit_char('\n');
        break;

    default:
        this->_real_commit_char(c);
        break;
    }
}

void tty::show_char(int c)
{
    this->putchar(c);
}

vga_tty::vga_tty(): tty{"ttyVGA"} { }

void vga_tty::putchar(char c)
{
    static struct vga_char vc = { .c = '\0', .color = VGA_CHAR_COLOR_WHITE };
    vc.c = c;
    vga_put_char(&vc);
}

void tty::clear_read_buf(void)
{
    this->buf.clear();
}

int kernel::tty::register_tty(tty* tty_dev)
{
    // TODO: manage all ttys
    if (!console)
        console = tty_dev;

    return 0;
}
