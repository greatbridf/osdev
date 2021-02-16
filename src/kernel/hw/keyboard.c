#include <asm/port_io.h>
#include <kernel/hw/keyboard.h>
#include <kernel/stdio.h>
#include <kernel/vga.h>
#include <types/types.h>

#define KEYBOARD_SCAN_CODE_BUFFER_SIZE (256)
static char keyboard_scan_code_buf[KEYBOARD_SCAN_CODE_BUFFER_SIZE];
static struct ring_buffer* p_scan_code_buf = 0;

void handle_keyboard_interrupt(void)
{
    static struct ring_buffer
        s_scan_code_buf
        = MAKE_RING_BUFFER(keyboard_scan_code_buf, KEYBOARD_SCAN_CODE_BUFFER_SIZE);
    if (p_scan_code_buf == 0)
        p_scan_code_buf = &s_scan_code_buf;

    char c = 0x00;
    c = (char)asm_inb(PORT_KEYDATA);

    ring_buffer_write(p_scan_code_buf, c);
}

int32_t keyboard_has_data(void)
{
    // uninitialized
    if (p_scan_code_buf == 0)
        return 0;
    return !ring_buffer_empty(p_scan_code_buf);
}

void process_keyboard_data(void)
{
    char buf[128] = { 0 };
    while (keyboard_has_data()) {
        char c;
        c = ring_buffer_read(p_scan_code_buf);
        snprintf(buf, 128, "%d", (int32_t)c);
    }
    vga_printk(buf, 0x0fu);
}
