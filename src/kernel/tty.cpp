#include <asm/port_io.h>
#include <kernel/hw/serial.h>
#include <kernel/mem.h>
#include <kernel/stdio.h>
#include <kernel/tty.h>
#include <kernel/vga.h>

static void serial_tty_put_char(struct tty* p_tty, char c)
{
    serial_send_data(p_tty->data.u16[0], c);
}

static void vga_tty_put_char(struct tty* _unused, char c)
{
    static struct vga_char vc = { .c = '\0', .color = VGA_CHAR_COLOR_WHITE };
    vc.c = c;
    vga_put_char(&vc);
}

static struct tty_operations serial_tty_ops = {
    .put_char = serial_tty_put_char,
};

static struct tty_operations vga_tty_ops = {
    .put_char = vga_tty_put_char,
};

void tty_print(struct tty* p_tty, const char* str)
{
    while (*str != '\0') {
        p_tty->ops->put_char(p_tty, *str);
        ++str;
    }
}

int make_serial_tty(struct tty* p_tty, int id, int buffered)
{
    // 0-1: port id
    // 2  : is_buffered
    // 3  : unused
    // 4-7: buffer pointer
    p_tty->data.u16[0] = id;
    p_tty->data.p[1] = buffered ? k_malloc(SERIAL_TTY_BUFFER_SIZE) : NULL;
    p_tty->data.u8[2] = buffered;
    snprintf(p_tty->name, sizeof(p_tty->name), "ttyS%x", id);
    p_tty->ops = &serial_tty_ops;
    return GB_OK;
}

int make_vga_tty(struct tty* p_tty)
{
    snprintf(p_tty->name, sizeof(p_tty->name), "ttyVGA");
    p_tty->ops = &vga_tty_ops;
    return GB_OK;
}
