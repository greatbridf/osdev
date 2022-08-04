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
#include <types/allocator.hpp>
#include <types/assert.h>
#include <types/bitmap.h>
#include <types/size.h>
#include <types/status.h>

// constant values

#define EMPTY_PAGE_ADDR ((pptr_t)0x0000)
#define EMPTY_PAGE_END ((pptr_t)0x1000)

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

    inline constexpr int brk(byte* addr)
    {
        if (unlikely(addr >= p_limit))
            return GB_FAILED;
        p_break = addr;
        return GB_OK;
    }

    // sets errno
    inline byte* sbrk(size_type increment)
    {
        if (unlikely(brk(p_break + increment) != GB_OK)) {
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
                if (unlikely(!start_pos->flags.has_next)) {
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
        if (unlikely(errno)) {
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
    void* ptr = kernel_heap_allocator->alloc(size);
    assert_likely(ptr);
    return ptr;
}

void k_free(void* ptr)
{
    kernel_heap_allocator->free(ptr);
}

void* ki_malloc(size_t size)
{
    void* ptr = kernel_ident_mapped_allocator.alloc(size);
    assert_likely(ptr);
    return ptr;
}

void ki_free(void* ptr)
{
    kernel_ident_mapped_allocator.free(ptr);
}

void* ptovp(pptr_t p_ptr)
{
    // memory below 768MiB is identically mapped
    // TODO: address translation for high mem
    assert(p_ptr <= 0x30000000);
    return (void*)p_ptr;
}

inline void mark_page(page_t n)
{
    bm_set(mem_bitmap, n);
}

inline void free_page(page_t n)
{
    bm_clear(mem_bitmap, n);
}

constexpr void mark_addr_len(pptr_t start, size_t n)
{
    if (unlikely(n == 0))
        return;
    page_t start_page = to_page(start);
    page_t end_page = to_page(start + n + 4095);
    for (page_t i = start_page; i < end_page; ++i)
        mark_page(i);
}

constexpr void free_addr_len(pptr_t start, size_t n)
{
    if (unlikely(n == 0))
        return;
    page_t start_page = to_page(start);
    page_t end_page = to_page(start + n + 4095);
    for (page_t i = start_page; i < end_page; ++i)
        free_page(i);
}

inline constexpr void mark_addr_range(pptr_t start, pptr_t end)
{
    mark_addr_len(start, end - start);
}

inline constexpr void free_addr_range(pptr_t start, pptr_t end)
{
    free_addr_len(start, end - start);
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
                mark_page(i);
            return first;
        }
    }
    assert(false);
    return 0xffffffff;
}

void free_n_raw_pages(page_t start_pg, size_t n)
{
    while (n--)
        free_page(start_pg++);
}

struct page allocate_page(void)
{
    return page {
        .phys_page_id = alloc_raw_page(),
        .pte = nullptr,
        .ref_count = types::_new<types::kernel_ident_allocator, size_t>(0),
        .attr { 0 },
    };
}

pd_t alloc_pd(void)
{
    // TODO: alloc page in low mem and gen struct page for it
    page_t pd_page = alloc_raw_page();
    pd_t pd = to_pd(pd_page);
    memset(pd, 0x00, PAGE_SIZE);
    return pd;
}

pt_t alloc_pt(void)
{
    // TODO: alloc page in low mem and gen struct page for it
    page_t pt_page = alloc_raw_page();
    pt_t pt = to_pt(pt_page);
    memset(pt, 0x00, PAGE_SIZE);
    return pt;
}

void dealloc_pd(pd_t pd)
{
    for (pde_t* ent = (*pd) + 256; ent < (*pd) + 1024; ++ent) {
        if (!ent->in.p)
            continue;
        dealloc_pt(to_pt(ent));
    }
    memset(pd, 0x00, sizeof(*pd));

    page_t pg = to_page((pptr_t)pd);
    free_page(pg);
}
void dealloc_pt(pt_t pt)
{
    memset(pt, 0x00, sizeof(*pt));

    page_t pg = to_page((pptr_t)pt);
    free_page(pg);
}

static inline void init_mem_layout(void)
{
    mem_size = 1024 * mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * mem_size_info.n_64k_blks;

    // mark kernel page directory
    mark_addr_range(0x00001000, 0x00006000);
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

using kernel::mm_list;
mm_list::mm_list(const mm_list& v)
    : m_areas(v.m_areas)
{
    pd_t pd = alloc_pd();
    memcpy(pd, v.m_pd, PAGE_SIZE);
    m_pd = pd;
}

inline void map_raw_page_to_pte(
    pte_t* pte,
    page_t page,
    bool present,
    bool write,
    bool priv)
{
    // set P bit
    pte->v = 0;
    pte->in.p = present;
    pte->in.rw = write;
    pte->in.us = !priv;
    pte->in.page = page;
}

int mm::append_page(page* pg, bool present, bool write, bool priv, bool cow)
{
    void* addr = this->end();
    pde_t* pde = to_pde(this->owner->m_pd, addr);
    // page table not exist
    if (unlikely(!pde->in.p)) {
        // allocate a page for the page table
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = 1;
        pde->in.pt_page = alloc_raw_page();

        memset(to_pt(pde), 0x00, PAGE_SIZE);
    }

    // map the page in the page table
    pte_t* pte = to_pte(pde, addr);
    map_raw_page_to_pte(pte, pg->phys_page_id, present, (write && !cow), priv);

    if (unlikely(cow && !pg->attr.in.cow)) {
        pg->attr.in.cow = 1;
        pg->pte->in.rw = 0;
        pg->pte->in.a = 0;
        invalidate_tlb(addr);
    }
    ++*pg->ref_count;

    auto iter = this->pgs->emplace_back(*pg);
    iter->pte = pte;
    return GB_OK;
}

static inline int _mmap(
    mm_list* mms,
    void* hint,
    size_t len,
    fs::inode* file,
    size_t offset,
    int write,
    int priv)
{
    if (unlikely(!file->flags.in.file && !file->flags.in.special_node)) {
        errno = EINVAL;
        return GB_FAILED;
    }

    len = (len + PAGE_SIZE - 1) & 0xfffff000;
    size_t n_pgs = len >> 12;

    for (const auto& mm_area : *mms)
        if (!mm_area.is_avail(hint, (char*)hint + len)) {
            errno = EEXIST;
            return GB_FAILED;
        }

    auto mm = mms->addarea(hint, write, priv);
    mm->mapped_file = file;
    mm->file_offset = offset;

    for (size_t i = 0; i < n_pgs; ++i)
        mm->append_page(&empty_page, false, write, priv, true);

    return GB_OK;
}

int mmap(
    void* hint,
    size_t len,
    fs::inode* file,
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
    pde_t* pde = *KERNEL_PAGE_DIRECTORY_ADDR + to_pdi(page);
    // page table not exist
    if (unlikely(!pde->in.p)) {
        // allocate a page for the page table
        // set the P bit of the pde in advance
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = 0;
        pde->in.pt_page = alloc_raw_page();
        _init_map_page_identically(pde->in.pt_page);
        memset(to_pt(pde), 0x00, PAGE_SIZE);
    }

    // map the page in the page table
    pte_t* pt = to_pte(pde, page);
    pt->v = 0x00000003;
    pt->in.page = page;
}

static inline void init_paging_map_low_mem_identically(void)
{
    for (pptr_t addr = 0x01000000; addr < 0x30000000; addr += 0x1000) {
        // check if the address is valid and not mapped
        if (bm_test(mem_bitmap, to_page(addr)))
            continue;
        _init_map_page_identically(to_page(addr));
    }
}

void init_mem(void)
{
    init_mem_layout();

    // map the 16MiB-768MiB identically
    init_paging_map_low_mem_identically();

    kernel_mms = types::pnew<types::kernel_ident_allocator>(kernel_mms, KERNEL_PAGE_DIRECTORY_ADDR);
    auto heap_mm = kernel_mms->addarea(KERNEL_HEAP_START, true, true);

    // create empty_page struct
    empty_page.attr.in.cow = 0;
    empty_page.phys_page_id = to_page(EMPTY_PAGE_ADDR);
    empty_page.ref_count = types::_new<types::kernel_ident_allocator, size_t>(1);
    empty_page.pte = to_pte(*KERNEL_PAGE_DIRECTORY_ADDR, empty_page.phys_page_id);
    empty_page.pte->in.rw = 0;
    invalidate_tlb(0x00000000);

    // 0x30000000 to 0x40000000 or 768MiB to 1GiB
    while (heap_mm->pgs->size() < 256 * 1024 * 1024 / PAGE_SIZE)
        heap_mm->append_page(&empty_page, true, true, true, true);

    kernel_heap_allocator = types::pnew<types::kernel_ident_allocator>(kernel_heap_allocator,
        KERNEL_HEAP_START, vptrdiff(KERNEL_HEAP_LIMIT, KERNEL_HEAP_START));
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
