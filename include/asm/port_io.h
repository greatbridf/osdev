#pragma once

#include <types/types.h>

#define PORT_KEYDATA 0x0060u

extern void asm_outb(uint16_t port_number, uint8_t data);
extern uint8_t asm_inb(uint16_t port_number);
