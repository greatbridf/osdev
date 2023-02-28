#include <kernel/hw/serial.h>
#include <kernel/stdio.hpp>
#include <kernel/tty.hpp>
#include <kernel/vga.hpp>
#include <types/stdint.h>

tty::tty()
    : buf(BUFFER_SIZE)
{
}

void tty::print(const char* str)
{
    while (*str != '\0')
        this->putchar(*(str++));
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
