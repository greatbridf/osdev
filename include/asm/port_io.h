#pragma once

#include <types/types.h>

typedef uint16_t port_id_t;

#define PORT_PIC1 (0x20)
#define PORT_PIC2 (0xa0)
#define PORT_PIC1_COMMAND (PORT_PIC1)
#define PORT_PIC1_DATA ((PORT_PIC1) + 1)
#define PORT_PIC2_COMMAND (PORT_PIC2)
#define PORT_PIC2_DATA ((PORT_PIC2) + 1)

#define PORT_KEYBOARD_COMMAND (0x64)
#define PORT_KEYBOARD_DATA (0x60)

#define PORT_PIT_CONTROL (0x43)
#define PORT_PIT_COUNT (0x40)

#define PORT_KEYDATA 0x0060u

#ifdef __cplusplus
extern "C" {
#endif

extern void asm_outb(port_id_t port_number, uint8_t data);
extern uint8_t asm_inb(port_id_t port_number);

extern void asm_hlt(void);
extern void asm_cli(void);
extern void asm_sti(void);

#ifdef __cplusplus
}
#endif
