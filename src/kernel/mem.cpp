#include <asm/boot.h>
#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/errno.h>
#include <kernel/mem.hpp>
#include <kernel/stdio.h>
#include <kernel/task.h>
#include <kernel/vga.h>
#include <kernel_main.h>
#include <types/bitmap.h>
#include <types/list.h>

// static variables

struct mm kernel_mm;
struct mm* kernel_mm_head;

// ---------------------

// constant values

#define EMPTY_PAGE_ADDR ((phys_ptr_t)0x5000)
#define EMPTY_PAGE_END ((phys_ptr_t)0x6000)

// ---------------------

static void* p_start;
static void* p_break;

static size_t mem_size;
static char mem_bitmap[1024 * 1024 / 8];

static int32_t set_heap_start(void* start_addr)
{
    p_start = start_addr;
    return 0;
}

static int32_t brk(void* addr)
{
    if (addr >= KERNEL_HEAP_LIMIT) {
        return GB_FAILED;
    }
    p_break = addr;
    return 0;
}

// sets errno when failed to increase heap pointer
static void* sbrk(size_t increment)
{
    if (brk((char*)p_break + increment) != 0) {
        errno = ENOMEM;
        return 0;
    } else {
        errno = 0;
        return p_break;
    }
}

int init_heap(void)
{
    set_heap_start(KERNEL_HEAP_START);

    if (brk(KERNEL_HEAP_START) != 0) {
        return GB_FAILED;
    }
    struct mem_blk* p_blk = (struct mem_blk*)sbrk(0);
    p_blk->size = 4;
    p_blk->flags.has_next = 0;
    p_blk->flags.is_free = 1;
    return GB_OK;
}

static inline struct mem_blk* _find_next_mem_blk(struct mem_blk* blk, size_t blk_size)
{
    char* p = (char*)blk;
    p += sizeof(struct mem_blk);
    p += blk_size;
    p -= (4 * sizeof(uint8_t));
    return (struct mem_blk*)p;
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
            start_pos = _find_next_mem_blk(start_pos, start_pos->size);
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

    struct mem_blk* blk = _find_next_mem_blk(blk_before, blk_before->size);

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

    struct mem_blk* blk_next = _find_next_mem_blk(blk, this_size);

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

    block_allocated = find_blk((struct mem_blk*)p_start, size);
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
    ptr = (void*)((char*)ptr - (sizeof(struct mem_blk_flags) + sizeof(size_t)));
    struct mem_blk* blk = (struct mem_blk*)ptr;
    blk->flags.is_free = 1;
    // TODO: fusion free blocks nearby
}

void* p_ptr_to_v_ptr(phys_ptr_t p_ptr)
{
    if (p_ptr <= 0x30000000) {
        // memory below 768MiB is identically mapped
        return (void*)p_ptr;
    } else {
        // TODO: address translation
        MAKE_BREAK_POINT();
        return (void*)0xffffffff;
    }
}

phys_ptr_t l_ptr_to_p_ptr(struct mm* mm, linr_ptr_t v_ptr)
{
    while (mm != NULL) {
        if (v_ptr < mm->start || v_ptr >= mm->start + mm->len * 4096) {
            mm = mm->next;
            continue;
        }
        size_t offset = (size_t)(v_ptr - mm->start);
        LIST_LIKE_AT(struct page, mm->pgs, offset / PAGE_SIZE, result);
        return page_to_phys_addr(result->phys_page_id) + (offset % 4096);
    }

    // TODO: handle error
    return 0xffffffff;
}

phys_ptr_t v_ptr_to_p_ptr(void* v_ptr)
{
    if (v_ptr < KERNEL_IDENTICALLY_MAPPED_AREA_LIMIT) {
        return (phys_ptr_t)v_ptr;
    }
    return l_ptr_to_p_ptr(kernel_mm_head, (linr_ptr_t)v_ptr);
}

static inline void mark_page(page_t n)
{
    bm_set(mem_bitmap, n);
}

static inline void free_page(page_t n)
{
    bm_clear(mem_bitmap, n);
}

static void mark_addr_len(phys_ptr_t start, size_t n)
{
    if (n == 0)
        return;
    page_t start_page = phys_addr_to_page(start);
    page_t end_page = phys_addr_to_page(start + n + 4095);
    for (page_t i = start_page; i < end_page; ++i)
        mark_page(i);
}

static void free_addr_len(phys_ptr_t start, size_t n)
{
    if (n == 0)
        return;
    page_t start_page = phys_addr_to_page(start);
    page_t end_page = phys_addr_to_page(start + n + 4095);
    for (page_t i = start_page; i < end_page; ++i)
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

page_t alloc_raw_page(void)
{
    for (page_t i = 0; i < 1024 * 1024; ++i) {
        if (bm_test(mem_bitmap, i) == 0) {
            mark_page(i);
            return i;
        }
    }
    MAKE_BREAK_POINT();
    return 0xffffffff;
}

struct page* allocate_page(void)
{
    // TODO: allocate memory on identically mapped area
    struct page* p = (struct page*)k_malloc(sizeof(struct page));
    memset(p, 0x00, sizeof(struct page));
    p->phys_page_id = alloc_raw_page();
    p->ref_count = (size_t*)k_malloc(sizeof(size_t));
    return p;
}

static inline void make_page_table(page_table_entry* pt)
{
    memset(pt, 0x00, sizeof(page_table_entry) * 1024);
}

static inline void init_mem_layout(void)
{
    mem_size = 1024 * mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * mem_size_info.n_64k_blks;

    // mark kernel page directory
    mark_addr_range(0x00000000, 0x00005000);
    // mark empty page
    mark_addr_range(EMPTY_PAGE_ADDR, EMPTY_PAGE_END);
    // mark EBDA and upper memory as allocated
    mark_addr_range(0x80000, 0xfffff);
    // mark kernel
    mark_addr_len(0x00100000, kernel_size);

    if (e820_mem_map_entry_size == 20) {
        struct e820_mem_map_entry_20* entry = (struct e820_mem_map_entry_20*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            if (entry->type != 1) {
                mark_addr_len(entry->base, entry->len);
            }
        }
    } else {
        struct e820_mem_map_entry_24* entry = (struct e820_mem_map_entry_24*)e820_mem_map;
        for (uint32_t i = 0; i < e820_mem_map_count; ++i, ++entry) {
            if (entry->in.type != 1) {
                mark_addr_len(entry->in.base, entry->in.len);
            }
        }
    }
}

int is_l_ptr_valid(struct mm* mm_area, linr_ptr_t l_ptr)
{
    while (mm_area != NULL) {
        if (l_ptr >= mm_area->start && l_ptr < mm_area->start + mm_area->len * PAGE_SIZE) {
            return GB_OK;
        }
        mm_area = mm_area->next;
    }
    return GB_FAILED;
}

struct page* find_page_by_l_ptr(struct mm* mm, linr_ptr_t l_ptr)
{
    if (mm == kernel_mm_head && l_ptr < (linr_ptr_t)KERNEL_IDENTICALLY_MAPPED_AREA_LIMIT) {
        // TODO: make mm for identically mapped area
        MAKE_BREAK_POINT();
        return (struct page*)0xffffffff;
    }
    while (mm != NULL) {
        if (l_ptr >= mm->start && l_ptr < mm->start + mm->len * 4096) {
            size_t offset = (size_t)(l_ptr - mm->start);
            LIST_LIKE_AT(struct page, mm->pgs, offset / PAGE_SIZE, result);
            return result;
        }
        mm = mm->next;
    }

    // TODO: error handling
    return NULL;
}

static inline void map_raw_page_to_pte(
    page_table_entry* pte,
    page_t page,
    int rw,
    int priv)
{
    // set P bit
    pte->v = 0x00000001;
    pte->in.rw = (rw == 1);
    pte->in.us = (priv == 1);
    pte->in.page = page;
}

static void _map_raw_page_to_addr(
    struct mm* mm_area,
    page_t page,
    int rw,
    int priv)
{
    linr_ptr_t addr = (linr_ptr_t)mm_area->start + mm_area->len * 4096;
    page_directory_entry* pde = mm_area->pd + linr_addr_to_pd_i(addr);
    // page table not exist
    if (!pde->in.p) {
        // allocate a page for the page table
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = 0;
        pde->in.pt_page = alloc_raw_page();

        make_page_table((page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page)));
    }

    // map the page in the page table
    page_table_entry* pte = (page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page));
    pte += linr_addr_to_pt_i(addr);
    map_raw_page_to_pte(pte, page, rw, priv);
}

// map page to the end of mm_area in pd
int k_map(
    struct mm* mm_area,
    struct page* page,
    int read,
    int write,
    int priv,
    int cow)
{
    struct page* p_page_end = mm_area->pgs;
    while (p_page_end != NULL && p_page_end->next != NULL)
        p_page_end = p_page_end->next;

    if (cow) {
        // find its ancestor
        while (page->attr.cow)
            page = page->next;

        // create a new page node
        struct page* new_page = (struct page*)k_malloc(sizeof(struct page));

        new_page->attr.read = (read == 1);
        new_page->attr.write = (write == 1);
        new_page->attr.system = (priv == 1);
        new_page->attr.cow = 1;
        // TODO: move *next out of struct page
        new_page->next = NULL;

        new_page->phys_page_id = page->phys_page_id;
        new_page->ref_count = page->ref_count;

        if (p_page_end != NULL)
            p_page_end->next = new_page;
        else
            mm_area->pgs = new_page;
    } else {
        page->attr.read = (read == 1);
        page->attr.write = (write == 1);
        page->attr.system = (priv == 1);
        page->attr.cow = 0;
        // TODO: move *next out of struct page
        page->next = NULL;

        if (p_page_end != NULL)
            p_page_end->next = page;
        else
            mm_area->pgs = page;
    }
    _map_raw_page_to_addr(
        mm_area,
        page->phys_page_id,
        (write && !cow),
        priv);

    ++mm_area->len;
    ++*page->ref_count;
    return GB_OK;
}

// map a page identically
// this function is only meant to be used in the initialization process
// it checks the pde's P bit so you need to make sure it's already set
// to avoid dead loops
static inline void _init_map_page_identically(page_t page)
{
    page_directory_entry* pde = KERNEL_PAGE_DIRECTORY_ADDR + page_to_pd_i(page);
    // page table not exist
    if (!pde->in.p) {
        // allocate a page for the page table
        // set the P bit of the pde in advance
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = 0;
        pde->in.pt_page = alloc_raw_page();
        _init_map_page_identically(pde->in.pt_page);

        make_page_table((page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page)));
    }

    // map the page in the page table
    page_table_entry* pt = (page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page));
    pt += page_to_pt_i(page);
    pt->v = 0x00000003;
    pt->in.page = page;
}

static inline void init_paging_map_low_mem_identically(void)
{
    for (phys_ptr_t addr = 0x01000000; addr < 0x30000000; addr += 0x1000) {
        // check if the address is valid and not mapped
        if (bm_test(mem_bitmap, phys_addr_to_page(addr)))
            continue;
        _init_map_page_identically(phys_addr_to_page(addr));
    }
}

static struct page empty_page;
static struct page heap_first_page;
static size_t heap_first_page_ref_count;

void init_mem(void)
{
    init_mem_layout();

    // map the 16MiB-768MiB identically
    init_paging_map_low_mem_identically();

    kernel_mm_head = &kernel_mm;

    kernel_mm.attr.read = 1;
    kernel_mm.attr.write = 1;
    kernel_mm.attr.system = 1;
    kernel_mm.len = 0;
    kernel_mm.next = NULL;
    kernel_mm.pd = KERNEL_PAGE_DIRECTORY_ADDR;
    kernel_mm.pgs = NULL;
    kernel_mm.start = (linr_ptr_t)KERNEL_HEAP_START;

    heap_first_page.attr.cow = 0;
    heap_first_page.attr.read = 1;
    heap_first_page.attr.write = 1;
    heap_first_page.attr.system = 1;
    heap_first_page.next = NULL;
    heap_first_page.phys_page_id = alloc_raw_page();
    heap_first_page.ref_count = &heap_first_page_ref_count;

    *heap_first_page.ref_count = 0;

    k_map(kernel_mm_head, &heap_first_page, 1, 1, 1, 0);

    init_heap();

    // create empty_page struct
    empty_page.attr.cow = 0;
    empty_page.attr.read = 1;
    empty_page.attr.write = 0;
    empty_page.attr.system = 0;
    empty_page.next = NULL;
    empty_page.phys_page_id = phys_addr_to_page(EMPTY_PAGE_ADDR);
    empty_page.ref_count = (size_t*)k_malloc(sizeof(size_t));
    *empty_page.ref_count = 1;

    // TODO: improve the algorithm SO FREAKING SLOW
    // while (kernel_mm_head->len < 256 * 1024 * 1024 / PAGE_SIZE) {
    while (kernel_mm_head->len < 16 * 1024 * 1024 / PAGE_SIZE) {
        k_map(
            kernel_mm_head, &empty_page,
            1, 1, 1, 1);
    }
}

void create_segment_descriptor(
    segment_descriptor* sd,
    uint32_t base,
    uint32_t limit,
    uint32_t flags,
    uint32_t access)
{
    sd->base_low = base & 0x0000ffff;
    sd->base_mid = ((base & 0x00ff0000) >> 16);
    sd->base_high = ((base & 0xff000000) >> 24);
    sd->limit_low = limit & 0x0000ffff;
    sd->limit_high = ((limit & 0x000f0000) >> 16);
    sd->access = access;
    sd->flags = flags;
}
