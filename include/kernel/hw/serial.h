#pragma once
#include <asm/port_io.h>

#define PORT_SERIAL0 (0x3f8)
#define PORT_SERIAL1 (0x2f8)

int32_t init_serial_port(port_id_t port);

int32_t is_serial_has_data(port_id_t port);
uint8_t serial_read_data(port_id_t port);

int32_t is_serial_ready_for_transmition(port_id_t port);
void serial_send_data(port_id_t port, uint8_t data);
