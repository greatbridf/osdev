#include <types/buffer.h>
#include <types/stdint.h>

int32_t
ring_buffer_empty(struct ring_buffer* buf)
{
    return (buf->count == 0);
}

int32_t
ring_buffer_full(struct ring_buffer* buf)
{
    return (buf->count == (size_t)(buf->buf_end_pos - buf->buf_start_pos + 1));
}

static inline void
ring_buffer_move_base_ptr_forward(struct ring_buffer* buf)
{
    if (buf->base == buf->buf_end_pos) {
        buf->base = buf->buf_start_pos;
    } else {
        ++buf->base;
    }
}

static inline void
ring_buffer_move_head_ptr_forward(struct ring_buffer* buf)
{
    if (buf->head == buf->buf_end_pos) {
        buf->head = buf->buf_start_pos;
    } else {
        ++buf->head;
    }
}

static inline char
ring_buffer_get_data(struct ring_buffer* buf)
{
    --buf->count;
    return *buf->base;
}

static inline void
ring_buffer_put_data(struct ring_buffer* buf, char c)
{
    *buf->head = c;
    ++buf->count;
}

char ring_buffer_read(struct ring_buffer* buf)
{
    if (ring_buffer_empty(buf)) {
        // TODO: set error flag
        return 0xff;
    }

    char c = ring_buffer_get_data(buf);

    ring_buffer_move_base_ptr_forward(buf);

    return c;
}

int32_t
ring_buffer_write(struct ring_buffer* buf, char c)
{
    if (ring_buffer_full(buf)) {
        // TODO: set error flag
        return 1;
    }

    ring_buffer_put_data(buf, c);

    ring_buffer_move_head_ptr_forward(buf);

    return 0;
}
