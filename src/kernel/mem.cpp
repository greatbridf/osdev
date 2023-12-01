#include <cstddef>

#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/task.h>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/bitmap.h>
#include <types/size.h>
#include <types/status.h>

// constant values

#define EMPTY_PAGE ((page_t)0)

// ---------------------

static size_t mem_size;
static uint8_t mem_bitmap[1024 * 1024 / 8];

// global
segment_descriptor gdt[6];

uint8_t e820_mem_map[1024];
uint32_t e820_mem_map_count;
uint32_t e820_mem_map_entry_size;
struct mem_size_info mem_size_info;

void* operator new(size_t sz)
{
    void* ptr = types::__allocator::m_palloc->alloc(sz);
    assert(ptr);
    return ptr;
}

void* operator new[](size_t sz)
{
    void* ptr = types::__allocator::m_palloc->alloc(sz);
    assert(ptr);
    return ptr;
}

void operator delete(void* ptr)
{
    types::__allocator::m_palloc->free(ptr);
}

void operator delete(void* ptr, size_t)
{
    types::__allocator::m_palloc->free(ptr);
}

void operator delete[](void* ptr)
{
    types::__allocator::m_palloc->free(ptr);
}

void operator delete[](void* ptr, size_t)
{
    types::__allocator::m_palloc->free(ptr);
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
    if (n == 0)
        return;
    page_t start_page = align_down<12>(start) >> 12;
    page_t end_page = align_up<12>(start + n) >> 12;
    for (page_t i = start_page; i < end_page; ++i)
        mark_page(i);
}

constexpr void free_addr_len(pptr_t start, size_t n)
{
    if (n == 0)
        return;
    page_t start_page = align_down<12>(start) >> 12;
    page_t end_page = align_up<12>(start + n) >> 12;
    for (page_t i = start_page; i < end_page; ++i)
        free_page(i);
}

constexpr void mark_addr_range(pptr_t start, pptr_t end)
{
    mark_addr_len(start, end - start);
}

constexpr void free_addr_range(pptr_t start, pptr_t end)
{
    free_addr_len(start, end - start);
}

page_t __alloc_raw_page(void)
{
    for (size_t i = 0; i < sizeof(mem_bitmap); ++i) {
        if (bm_test(mem_bitmap, i) == 0) {
            bm_set(mem_bitmap, i);
            return i;
        }
    }
    return -1;
}

void __free_raw_page(page_t pg)
{
    bm_clear(mem_bitmap, pg);
}

page allocate_page(void)
{
    return page {
        .phys_page_id = __alloc_raw_page(),
        .ref_count = types::_new<types::kernel_ident_allocator, size_t>(0),
        .pg_pteidx = 0,
        .attr = 0,
    };
}

void free_page(page* pg)
{
    if (*pg->ref_count == 1) {
        types::pdelete<types::kernel_ident_allocator>(pg->ref_count);
        __free_raw_page(pg->phys_page_id);
    } else {
        --*pg->ref_count;
    }
}

void dealloc_pd(page_t pd)
{
    {
        kernel::paccess pa(pd);
        auto p_pd = (pd_t)pa.ptr();
        assert(p_pd);
        for (pde_t* ent = (*p_pd); ent < (*p_pd) + 768; ++ent) {
            if (!ent->in.p)
                continue;
            __free_raw_page(ent->in.pt_page);
        }
    }
    __free_raw_page(pd);
}

SECTION(".text.kinit")
static inline void init_mem_layout(void)
{
    mem_size = 1024 * mem_size_info.n_1k_blks;
    mem_size += 64 * 1024 * mem_size_info.n_64k_blks;

    // mark empty page
    mark_addr_range(0x00000000, 0x00001000);
    // mark kernel page directory
    mark_addr_range(0x00001000, 0x00002000);
    // mark kernel page table
    mark_addr_range(0x00002000, 0x00006000);
    // mark kernel early stack
    mark_addr_range(0x00006000, 0x00008000);
    // mark EBDA and upper memory as allocated
    mark_addr_range(0x80000, 0x100000);
    extern char __stage1_start[];
    extern char __kinit_end[];
    extern char __text_start[];
    extern char __data_end[];

    constexpr pptr_t PHYS_BSS_START = 0x100000;
    // mark .stage1 and .kinit
    mark_addr_range((pptr_t)__stage1_start, (pptr_t)__kinit_end);
    // mark kernel .text to .data
    mark_addr_len((pptr_t)__kinit_end, __data_end - __text_start);
    // mark kernel .bss
    mark_addr_len(PHYS_BSS_START, bss_len);

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
    m_pd = __alloc_raw_page();
    kernel::paccess pdst(m_pd), psrc(v.m_pd);
    auto* dst = pdst.ptr();
    auto* src = psrc.ptr();
    assert(dst && src);
    memcpy(dst, src, PAGE_SIZE);
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

int mm::append_page(page& pg, uint32_t attr, bool priv)
{
    void* addr = this->end();
    kernel::paccess pa(this->owner->m_pd);
    auto pd = (pd_t)pa.ptr();
    assert(pd);
    pde_t* pde = *pd + v_to_pdi(addr);

    page_t pt_pg = 0;
    pte_t* pte = nullptr;
    // page table not exist
    if (unlikely(!pde->in.p)) {
        // allocate a page for the page table
        pt_pg = __alloc_raw_page();
        pde->in.p = 1;
        pde->in.rw = 1;
        pde->in.us = 1;
        pde->in.pt_page = pt_pg;

        auto pt = (pt_t)kernel::pmap(pt_pg);
        assert(pt);
        pte = *pt;

        memset(pt, 0x00, PAGE_SIZE);
    } else {
        pt_pg = pde->in.pt_page;
        auto pt = (pt_t)kernel::pmap(pt_pg);
        assert(pt);
        pte = *pt;
    }

    // map the page in the page table
    int pti = v_to_pti(addr);
    pte += pti;

    map_raw_page_to_pte(
        pte,
        pg.phys_page_id,
        !(attr & PAGE_MMAP),
        false,
        priv);

    kernel::pfree(pt_pg);

    if (unlikely((attr & PAGE_COW) && !(pg.attr & PAGE_COW))) {
        kernel::paccess pa(pg.pg_pteidx >> 12);
        auto* pg_pte = (pte_t*)pa.ptr();
        assert(pg_pte);
        pg_pte += (pg.pg_pteidx & 0xfff);
        pg.attr |= PAGE_COW;
        pg_pte->in.rw = 0;
        pg_pte->in.a = 0;
        invalidate_tlb(addr);
    }
    ++*pg.ref_count;

    auto iter = this->pgs->emplace_back(pg);
    iter->pg_pteidx = (pt_pg << 12) + pti;
    iter->attr = attr;

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
    auto& mms = current_process->mms;

    if (unlikely(!file->flags.in.file && !file->flags.in.special_node)) {
        errno = EINVAL;
        return GB_FAILED;
    }

    // TODO: find another address
    assert(((uint32_t)hint & 0xfff) == 0);
    // TODO: return failed
    assert((offset & 0xfff) == 0);

    size_t n_pgs = align_up<12>(len) >> 12;

    if (!mms.is_avail(hint, len)) {
        errno = EEXIST;
        return GB_FAILED;
    }

    auto mm = mms.addarea(hint, write, priv);
    mm->mapped_file = file;
    mm->file_offset = offset;

    for (size_t i = 0; i < n_pgs; ++i)
        mm->append_page(empty_page, PAGE_MMAP | PAGE_COW, priv);

    return GB_OK;
}

SECTION(".text.kinit")
void init_mem(void)
{
    init_mem_layout();

    // TODO: replace early kernel pd
    kernel_mms = types::pnew<types::kernel_ident_allocator>(kernel_mms, EARLY_KERNEL_PD_PAGE);
    auto heap_mm = kernel_mms->addarea(KERNEL_HEAP_START, true, true);

    // create empty_page struct
    empty_page.attr = 0;
    empty_page.phys_page_id = EMPTY_PAGE;
    empty_page.ref_count = types::_new<types::kernel_ident_allocator, size_t>(2);
    empty_page.pg_pteidx = 0x00002000;

    // 0xd0000000 to 0xd4000000 or 3.5GiB, size 64MiB
    while (heap_mm->pgs->size() < 64 * 1024 * 1024 / PAGE_SIZE)
        heap_mm->append_page(empty_page, PAGE_COW, true);

    types::__allocator::init_kernel_heap(KERNEL_HEAP_START,
        vptrdiff(KERNEL_HEAP_LIMIT, KERNEL_HEAP_START));
}

SECTION(".text.kinit")
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

namespace __physmapper {
struct mapped_area {
    size_t ref;
    void* ptr;
};

static types::hash_map<page_t, mapped_area,
    types::linux_hasher, types::kernel_ident_allocator>
    mapped;
static uint8_t freebm[0x400 / 8];
} // namespace __physmapper

void* kernel::pmap(page_t pg)
{
    auto* const pmap_pt = std::bit_cast<pte_t*>(0xff001000);
    auto* const mapped_start = std::bit_cast<void*>(0xff000000);

    auto iter = __physmapper::mapped.find(pg);
    if (iter) {
        ++iter->value.ref;
        return iter->value.ptr;
    }

    for (int i = 2; i < 0x400; ++i) {
        if (bm_test(__physmapper::freebm, i) == 0) {
            auto* pte = pmap_pt + i;
            pte->v = 0x3;
            pte->in.page = pg;

            void* ptr = vptradd(mapped_start, 0x1000 * i);
            invalidate_tlb(ptr);

            bm_set(__physmapper::freebm, i);
            __physmapper::mapped.emplace(pg,
                __physmapper::mapped_area { 1, ptr });
            return ptr;
        }
    }

    return nullptr;
}
void kernel::pfree(page_t pg)
{
    auto* const pmap_pt = std::bit_cast<pte_t*>(0xff001000);
    auto* const mapped_start = std::bit_cast<void*>(0xff000000);

    auto iter = __physmapper::mapped.find(pg);
    if (!iter)
        return;
    auto& [ ref, ptr ] = iter->value;

    if (ref > 1) {
        --ref;
        return;
    }

    int i = vptrdiff(ptr, mapped_start);
    i /= 0x1000;

    auto* pte = pmap_pt + i;
    pte->v = 0;
    invalidate_tlb(ptr);

    bm_clear(__physmapper::freebm, i);
    __physmapper::mapped.remove(iter);
}
