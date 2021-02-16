#pragma once

#include "stdint.h"

struct ring_buffer {
    char* const buf_start_pos;
    char* const buf_end_pos;
    char* base;
    char* head;
    size_t count;
};

int32_t
ring_buffer_empty(struct ring_buffer* buf);

int32_t
ring_buffer_full(struct ring_buffer* buf);

char ring_buffer_read(struct ring_buffer* buf);

int32_t
ring_buffer_write(struct ring_buffer* buf, char c);
