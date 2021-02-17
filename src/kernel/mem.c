#include <asm/port_io.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/vga.h>
#include <kernel_main.h>

static void* p_start;
static void* p_break;

static int32_t set_heap_start(void* start_addr)
{
    p_start = start_addr;
    return 0;
}

static int32_t brk(void* addr)
{
    p_break = addr;
    return 0;
}

// sets errno when failed to increase heap pointer
static void* sbrk(size_t increment)
{
    if (brk(p_break + increment) != 0) {
        errno = ENOMEM;
        return 0;
    } else {
        errno = 0;
        return p_break;
    }
}

void init_heap(void)
{
    // start of the available address space
    // TODO: adjust heap start address
    //   according to user's memory size
    set_heap_start(HEAP_START);

    if (brk(HEAP_START) != 0) {
        vga_printk("Failed to initialize heap, halting...", 0x0fu);
        MAKE_BREAK_POINT();
        asm_cli();
        asm_hlt();
    }
    struct mem_blk* p_blk = sbrk(0);
    p_blk->size = 4;
    p_blk->flags.has_next = 0;
    p_blk->flags.is_free = 1;
}

// @param start_pos position where to start finding
// @param size the size of the block we're looking for
// @return found block if suitable block exists, if not, the last block
static struct mem_blk*
find_blk(
    struct mem_blk* start_pos,
    size_t size)
{
    while (1) {
        if (start_pos->flags.is_free && start_pos->size >= size) {
            errno = 0;
            return start_pos;
        } else {
            if (!start_pos->flags.has_next) {
                errno = ENOTFOUND;
                return start_pos;
            }
            start_pos = ((void*)start_pos)
                + sizeof(struct mem_blk)
                + start_pos->size
                - 4 * sizeof(uint8_t);
        }
    }
}

static struct mem_blk*
allocate_new_block(
    struct mem_blk* blk_before,
    size_t size)
{
    sbrk(sizeof(struct mem_blk) + size - 4 * sizeof(uint8_t));
    if (errno) {
        return 0;
    }

    struct mem_blk* blk = ((void*)blk_before)
        + sizeof(struct mem_blk)
        + blk_before->size
        - 4 * sizeof(uint8_t);

    blk_before->flags.has_next = 1;

    blk->flags.has_next = 0;
    blk->flags.is_free = 1;
    blk->size = size;

    errno = 0;
    return blk;
}

void* k_malloc(size_t size)
{
    struct mem_blk* block_allocated;

    block_allocated = find_blk(p_start, size);
    if (errno == ENOTFOUND) {
        // 'block_allocated' in the argument list is the pointer
        // pointing to the last block
        block_allocated = allocate_new_block(block_allocated, size);
        // no need to check errno and return value
        // preserve these for the caller
    } else {
        // TODO: split block
    }

    block_allocated->flags.is_free = 0;
    return block_allocated->data;
}
