#include <asm/boot.h>
#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/task.h>
#include <kernel/vga.h>
#include <kernel_main.h>
#include <types/bitmap.h>

static void* p_start;
static void* p_break;
static segment_descriptor* gdt;

// temporary
static struct tss32_t _tss;

static size_t mem_size;
static char mem_bitmap[1024 * 1024 / 8];

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

int init_heap(void)
{
    // start of the available address space
    // TODO: adjust heap start address
    //   according to user's memory size
    set_heap_start(HEAP_START);

    if (brk(HEAP_START) != 0) {
        return GB_FAILED;
    }
    struct mem_blk* p_blk = sbrk(0);
    p_blk->size = 4;
    p_blk->flags.has_next = 0;
    p_blk->flags.is_free = 1;
    return GB_OK;
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

static void split_block(
    struct mem_blk* blk,
    size_t this_size)
{
    // block is too small to get split
    if (blk->size < sizeof(struct mem_blk) + this_size) {
        return;
    }

    struct mem_blk* blk_next = ((void*)blk)
        + sizeof(struct mem_blk)
        + this_size
        - 4 * sizeof(uint8_t);

    blk_next->size = blk->size
        - this_size
        - sizeof(struct mem_blk)
        + 4 * sizeof(uint8_t);

    blk_next->flags.has_next = blk->flags.has_next;
    blk_next->flags.is_free = 1;

    blk->flags.has_next = 1;
    blk->size = this_size;
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
        split_block(block_allocated, size);
    }

    block_allocated->flags.is_free = 0;
    return block_allocated->data;
}

void k_free(void* ptr)
{
    ptr -= (sizeof(struct mem_blk_flags) + sizeof(size_t));
    struct mem_blk* blk = (struct mem_blk*)ptr;
    blk->flags.is_free = 1;
    // TODO: fusion free blocks nearby
}

static inline size_t addr_to_page(phys_ptr_t p)
{
    return p >> 12;
}

static inline void mark_page(size_t n)
{
    bm_set(mem_bitmap, n);
}

static inline void free_page(size_t n)
{
    bm_clear(mem_bitmap, n);
}

static void mark_addr_len(phys_ptr_t start, size_t n)
{
    if (n == 0) return;
    size_t start_page = addr_to_page(start);
    size_t end_page   = addr_to_page(start + n + 4095);
    for (uint32_t i = start_page; i < end_page; ++i)
        mark_page(i);
}

static void free_addr_len(phys_ptr_t start, size_t n)
{
    if (n == 0) return;
    size_t start_page = (start >> 12);
    size_t end_page   = ((start + n + 4095) >> 12);
    for (uint32_t i = start_page; i < end_page; ++i)
        free_page(i);
}

static inline void mark_addr_range(phys_ptr_t start, phys_ptr_t end)
{
    mark_addr_len(start, end - start);
}

static inline void free_addr_range(phys_ptr_t start, phys_ptr_t end)
{
    free_addr_len(start, end - start);
}

static int alloc_page(void)
{
    for (size_t i = 0; i < 1024 * 1024; ++i) {
        if (bm_test(mem_bitmap, i) == 0) {
            mark_page(i);
            return i;
        }
    }
    return GB_FAILED;
}

// allocate ONE whole page
static phys_ptr_t _k_p_malloc(void)
{
    return (phys_ptr_t)(alloc_page() << 12);
}

static void _k_p_free(phys_ptr_t ptr)
{
    free_page(addr_to_page(ptr));
}

static inline void _create_pd(page_directory_entry* pde)
{
}

static page_directory_entry* _kernel_pd = KERNEL_PAGE_DIRECTORY_ADDR;

static inline void _create_kernel_pt(page_table_entry* pt, int32_t index)
{
    // 0x00000000 ~ 0x3fffffff is mapped as kernel space
    // from physical address 0 to
    int32_t is_kernel = (index < 256);

    for (int32_t i = 0; i < 1024; ++i) {
        if (is_kernel) {
            pt[i].v = 0b00000011;
        } else {
            pt[i].v = 0b00000111;
        }
        pt[i].in.addr = ((index * 0x400000) + i * 0x1000) >> 12;
    }
}

static inline void _create_kernel_pd(void)
{
    for (int32_t i = 0; i < 1024; ++i) {
        if (i < 256) {
            _kernel_pd[i].v = 0b00000011;
        } else {
            _kernel_pd[i].v = 0b00000111;
        }
        page_table_entry* pt = (page_table_entry*)_k_p_malloc();
        _kernel_pd[i].in.addr = ((size_t)pt >> 12);
        _create_kernel_pt(pt, i);
    }
}

static void init_mem_layout(void)
{
    mem_size = 1024 * mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * mem_size_info.n_64k_blks;

    // mark kernel page directory
    mark_addr_range(0x00000000, 0x00001000);
    // mark EBDA and upper memory as allocated
    mark_addr_range(0x80000, 0xfffff);
    // mark kernel
    mark_addr_len(0x00100000, kernel_size);

    if (e820_mem_map_entry_size == 20) {
        struct e820_mem_map_entry_20* entry = (struct e820_mem_map_entry_20*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            if (entry->type != 1)
            {
                mark_addr_len(entry->base, entry->len);
            }
        }
    } else {
        struct e820_mem_map_entry_24* entry = (struct e820_mem_map_entry_24*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            if (entry->in.type != 1)
            {
                mark_addr_len(entry->in.base, entry->in.len);
            }
        }
    }
}

void init_paging(void)
{
    init_mem_layout();

    _create_kernel_pd();
    asm_enable_paging(_kernel_pd);
}

static inline void
set_segment_descriptor(
    segment_descriptor* sd,
    uint32_t base,
    uint32_t limit,
    uint8_t access,
    uint8_t flags)
{
    sd->access = access;
    sd->flags = flags;
    sd->base_low = base;
    sd->base_mid = base >> 16;
    sd->base_high = base >> 24;
    sd->limit_low = limit;
    sd->limit_high = limit >> 16;
}

void init_gdt_with_tss(void* kernel_esp, uint16_t kernel_ss)
{
    // TODO: fix this
    return;
    gdt = k_malloc(sizeof(segment_descriptor) * 6);
    // since the size in the struct is an OFFSET
    // it needs to be added one to get its real size
    uint16_t asm_gdt_size = (asm_gdt_descriptor.size + 1) / 8;
    segment_descriptor* asm_gdt = (segment_descriptor*)asm_gdt_descriptor.address;

    for (int i = 0; i < asm_gdt_size; ++i) {
        gdt[i] = asm_gdt[i];
    }

    set_segment_descriptor(gdt + 5, (uint32_t)&_tss, sizeof(struct tss32_t), SD_TYPE_TSS, 0b0000);

    _tss.esp0 = (uint32_t)kernel_esp;
    _tss.ss0 = kernel_ss;

    asm_load_gdt((6 * sizeof(segment_descriptor) - 1) << 16, (uint32_t)gdt);
    asm_load_tr((6 - 1) * 8);
}
