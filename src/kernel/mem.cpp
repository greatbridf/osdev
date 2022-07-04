#include <asm/boot.h>
#include <asm/port_io.h>
#include <asm/sys.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/stdio.h>
#include <kernel/task.h>
#include <kernel/vga.h>
#include <kernel_main.h>
#include <types/bitmap.h>
#include <types/status.h>

// global objects

mm_list* kernel_mms;

// ---------------------

// constant values

#define EMPTY_PAGE_ADDR ((phys_ptr_t)0x5000)
#define EMPTY_PAGE_END ((phys_ptr_t)0x6000)

#define IDENTICALLY_MAPPED_HEAP_SIZE ((size_t)0x400000)

// ---------------------

static size_t mem_size;
static char mem_bitmap[1024 * 1024 / 8];

class brk_memory_allocator {
public:
    using byte = uint8_t;
    using size_type = size_t;

    struct mem_blk_flags {
        uint8_t is_free;
        uint8_t has_next;
        uint8_t _unused2;
        uint8_t _unused3;
    };

    struct mem_blk {
        size_t size;
        struct mem_blk_flags flags;
        // the first byte of the memory space
        // the minimal allocated space is 4 bytes
        uint8_t data[4];
    };

private:
    byte* p_start;
    byte* p_break;
    byte* p_limit;

    brk_memory_allocator(void) = delete;
    brk_memory_allocator(const brk_memory_allocator&) = delete;
    brk_memory_allocator(brk_memory_allocator&&) = delete;

    inline int brk(byte* addr)
    {
        if (addr >= p_limit)
            return GB_FAILED;
        p_break = addr;
        return GB_OK;
    }

    // sets errno
    inline byte* sbrk(size_type increment)
    {
        if (brk(p_break + increment) != GB_OK) {
            errno = ENOMEM;
            return nullptr;
        } else {
            errno = 0;
            return p_break;
        }
    }

    inline mem_blk* _find_next_mem_blk(mem_blk* blk, size_type blk_size)
    {
        byte* p = (byte*)blk;
        p += sizeof(mem_blk);
        p += blk_size;
        p -= (4 * sizeof(byte));
        return (mem_blk*)p;
    }

    // sets errno
    // @param start_pos position where to start finding
    // @param size the size of the block we're looking for
    // @return found block if suitable block exists, if not, the last block
    mem_blk* find_blk(mem_blk* start_pos, size_type size)
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

    // sets errno
    mem_blk* allocate_new_block(mem_blk* blk_before, size_type size)
    {
        sbrk(sizeof(mem_blk) + size - 4 * sizeof(byte));
        // preserves errno
        if (errno) {
            return nullptr;
        }

        mem_blk* blk = _find_next_mem_blk(blk_before, blk_before->size);

        blk_before->flags.has_next = 1;

        blk->flags.has_next = 0;
        blk->flags.is_free = 1;
        blk->size = size;

        errno = 0;
        return blk;
    }

    void split_block(mem_blk* blk, size_type this_size)
    {
        // block is too small to get split
        if (blk->size < sizeof(mem_blk) + this_size) {
            return;
        }

        mem_blk* blk_next = _find_next_mem_blk(blk, this_size);

        blk_next->size = blk->size
            - this_size
            - sizeof(mem_blk)
            + 4 * sizeof(byte);

        blk_next->flags.has_next = blk->flags.has_next;
        blk_next->flags.is_free = 1;

        blk->flags.has_next = 1;
        blk->size = this_size;
    }

public:
    brk_memory_allocator(void* start, size_type limit)
        : p_start((byte*)start)
        , p_limit(p_start + limit)
    {
        brk(p_start);
        mem_blk* p_blk = (mem_blk*)sbrk(0);
        p_blk->size = 4;
        p_blk->flags.has_next = 0;
        p_blk->flags.is_free = 1;
    }

    // sets errno
    void* alloc(size_type size)
    {
        struct mem_blk* block_allocated;

        block_allocated = find_blk((mem_blk*)p_start, size);
        if (errno == ENOTFOUND) {
            // 'block_allocated' in the argument list is the pointer
            // pointing to the last block
            block_allocated = allocate_new_block(block_allocated, size);
            if (errno) {
                // preserves errno
                return nullptr;
            }
        } else {
            split_block(block_allocated, size);
        }

        errno = 0;
        block_allocated->flags.is_free = 0;
        return block_allocated->data;
    }

    void free(void* ptr)
    {
        mem_blk* blk = (mem_blk*)((byte*)ptr - (sizeof(mem_blk_flags) + sizeof(size_t)));
        blk->flags.is_free = 1;
        // TODO: fusion free blocks nearby
    }
};

static brk_memory_allocator* kernel_heap_allocator;
static brk_memory_allocator
    kernel_ident_mapped_allocator((void*)bss_section_end_addr,
        IDENTICALLY_MAPPED_HEAP_SIZE);

void* k_malloc(size_t size)
{
    return kernel_heap_allocator->alloc(size);
}

void k_free(void* ptr)
{
    kernel_heap_allocator->free(ptr);
}

void* ki_malloc(size_t size)
{
    void* ptr = kernel_ident_mapped_allocator.alloc(size);
    if (!ptr) {
        MAKE_BREAK_POINT();
    }
    return ptr;
}

void ki_free(void* ptr)
{
    kernel_ident_mapped_allocator.free(ptr);
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

phys_ptr_t l_ptr_to_p_ptr(const mm_list* mms, linr_ptr_t v_ptr)
{
    for (const mm& item : *mms) {
        if (v_ptr < item.start || v_ptr >= item.start + item.pgs->size() * PAGE_SIZE)
            continue;
        size_t offset = (size_t)(v_ptr - item.start);
        const page& p = item.pgs->at(offset / PAGE_SIZE);
        return page_to_phys_addr(p.phys_page_id) + (offset % PAGE_SIZE);
    }

    // TODO: handle error
    return 0xffffffff;
}

phys_ptr_t v_ptr_to_p_ptr(const void* v_ptr)
{
    if (v_ptr < KERNEL_IDENTICALLY_MAPPED_AREA_LIMIT) {
        return (phys_ptr_t)v_ptr;
    }
    return l_ptr_to_p_ptr(kernel_mms, (linr_ptr_t)v_ptr);
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
    return alloc_n_raw_pages(1);
}

// @return the max count (but less than n) of the pages continuously available
static inline size_t _test_n_raw_pages(page_t start, size_t n)
{
    // *start is already allocated
    if (bm_test(mem_bitmap, start))
        return 0;

    return 1 + ((n > 1) ? _test_n_raw_pages(start + 1, n - 1) : 0);
}

page_t alloc_n_raw_pages(size_t n)
{
    page_t first = 0;
    while (first <= 1024 * 1024 - n) {
        size_t max = _test_n_raw_pages(first, n);
        if (max != n) {
            first += (max + 1);
        } else {
            for (page_t i = first; i < first + n; ++i)
                bm_set(mem_bitmap, i);
            return first;
        }
    }
    MAKE_BREAK_POINT();
    return 0xffffffff;
}

struct page allocate_page(void)
{
    struct page p { };
    p.phys_page_id = alloc_raw_page();
    p.ref_count = types::kernel_ident_allocator_new<size_t>(0);
    return p;
}

static inline void make_page_table(page_table_entry* pt)
{
    memset(pt, 0x00, sizeof(page_table_entry) * 1024);
}

page_directory_entry* alloc_pd(void)
{
    // TODO: alloc page in low mem and gen struct page for it
    page_t pd_page = alloc_raw_page();
    page_directory_entry* pd = (page_directory_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pd_page));
    memset(pd, 0x00, PAGE_SIZE);
    return pd;
}

page_table_entry* alloc_pt(void)
{
    // TODO: alloc page in low mem and gen struct page for it
    page_t pt_page = alloc_raw_page();
    page_table_entry* pt = (page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pt_page));
    make_page_table(pt);
    return pt;
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
    // mark identically mapped heap
    mark_addr_len(bss_section_end_addr, IDENTICALLY_MAPPED_HEAP_SIZE);

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

mm* find_mm_area(mm_list* mms, linr_ptr_t l_ptr)
{
    for (auto iter = mms->begin(); iter != mms->end(); ++iter)
        if (l_ptr >= iter->start && l_ptr < iter->start + iter->pgs->size() * PAGE_SIZE)
            return iter.ptr();
    return nullptr;
}

struct page* find_page_by_l_ptr(const mm_list* mms, linr_ptr_t l_ptr)
{
    for (const mm& item : *mms) {
        if (l_ptr >= item.start && l_ptr < item.start + item.pgs->size() * PAGE_SIZE) {
            size_t offset = (size_t)(l_ptr - item.start);
            return &item.pgs->at(offset / PAGE_SIZE);
        }
    }

    // TODO: error handling
    return nullptr;
}

static inline void map_raw_page_to_pte(
    page_table_entry* pte,
    page_t page,
    int present,
    int rw,
    int priv)
{
    // set P bit
    pte->v = 0;
    pte->in.p = present;
    pte->in.rw = (rw == 1);
    pte->in.us = (priv == 0);
    pte->in.page = page;
}

// map page to the end of mm_area in pd
int k_map(
    mm* mm_area,
    const struct page* page,
    int read,
    int write,
    int priv,
    int cow)
{
    linr_ptr_t addr = (linr_ptr_t)mm_area->start + mm_area->pgs->size() * PAGE_SIZE;
    page_directory_entry* pde = mm_area->pd + linr_addr_to_pd_i(addr);
    // page table not exist
    if (!pde->in.p) {
        // allocate a page for the page table
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = (priv == 0);
        pde->in.pt_page = alloc_raw_page();

        make_page_table((page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page)));
    }

    // map the page in the page table
    page_table_entry* pte = (page_table_entry*)p_ptr_to_v_ptr(page_to_phys_addr(pde->in.pt_page));
    pte += linr_addr_to_pt_i(addr);
    map_raw_page_to_pte(pte, page->phys_page_id, read, (write && !cow), priv);

    mm_area->pgs->push_back(*page);
    mm_area->pgs->back()->attr.cow = cow;
    ++*page->ref_count;
    return GB_OK;
}

bool check_addr_range_avail(const mm* mm_area, void* start, void* end)
{
    void* m_start = (void*)mm_area->start;
    void* m_end = (void*)(mm_area->start + PAGE_SIZE * mm_area->pgs->size());

    if (start >= m_end || end <= m_start)
        return true;
    else
        return false;
}

static inline int _mmap(
    mm_list* mms,
    void* hint,
    size_t len,
    struct inode* file,
    size_t offset,
    int write,
    int priv)
{
    if (!file->flags.in.file && !file->flags.in.special_node) {
        errno = EINVAL;
        return GB_FAILED;
    }

    len = (len + PAGE_SIZE - 1) & 0xfffff000;
    size_t n_pgs = len >> 12;

    for (const auto& mm_area : *mms)
        if (!check_addr_range_avail(&mm_area, hint, (char*)hint + len)) {
            errno = EEXIST;
            return GB_FAILED;
        }

    auto iter_mm = mms->emplace_back((linr_ptr_t)hint, mms_get_pd(&current_process->mms), write, priv);
    iter_mm->mapped_file = file;
    iter_mm->file_offset = offset;

    for (size_t i = 0; i < n_pgs; ++i)
        k_map(iter_mm.ptr(), &empty_page, 0, write, priv, 1);

    return GB_OK;
}

int mmap(
    void* hint,
    size_t len,
    struct inode* file,
    size_t offset,
    int write,
    int priv)
{
    return _mmap(&current_process->mms, hint, len, file, offset, write, priv);
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

page empty_page;

void init_mem(void)
{
    init_mem_layout();

    // map the 16MiB-768MiB identically
    init_paging_map_low_mem_identically();

    kernel_mms = types::kernel_ident_allocator_new<mm_list>();
    auto heap_mm = kernel_mms->emplace_back((linr_ptr_t)KERNEL_HEAP_START, KERNEL_PAGE_DIRECTORY_ADDR, 1, 1);

    page heap_first_page {
        .phys_page_id = alloc_raw_page(),
        .ref_count = types::kernel_ident_allocator_new<size_t>(0),
        .attr = {
            .cow = 0,
        },
    };

    k_map(heap_mm.ptr(), &heap_first_page, 1, 1, 1, 0);
    memset(KERNEL_HEAP_START, 0x00, PAGE_SIZE);
    kernel_heap_allocator = types::kernel_ident_allocator_new<brk_memory_allocator>(KERNEL_HEAP_START,
        (uint32_t)KERNEL_HEAP_LIMIT - (uint32_t)KERNEL_HEAP_START);

    // create empty_page struct
    empty_page.attr.cow = 0;
    empty_page.phys_page_id = phys_addr_to_page(EMPTY_PAGE_ADDR);
    empty_page.ref_count = types::kernel_ident_allocator_new<size_t>(1);

    // TODO: improve the algorithm SO FREAKING SLOW
    // while (kernel_mm_head->len < 256 * 1024 * 1024 / PAGE_SIZE) {
    while (heap_mm->pgs->size() < 256 * 1024 * 1024 / PAGE_SIZE) {
        k_map(
            heap_mm.ptr(), &empty_page,
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

mm::mm(linr_ptr_t start, page_directory_entry* pd, bool write, bool system)
    : start(start)
    , attr({
          .read { 1 },
          .write { write },
          .system { system },
      })
    , pd(pd)
    , pgs(types::kernel_ident_allocator_new<page_arr>())
    , mapped_file(nullptr)
    , file_offset(0)
{
}

mm::mm(const mm& val)
    : start(val.start)
    , attr({
          .read { val.attr.read },
          .write { val.attr.write },
          .system { val.attr.system },
      })
    , pd(val.pd)
    , pgs(val.pgs)
    , mapped_file(nullptr)
    , file_offset(0)
{
}
