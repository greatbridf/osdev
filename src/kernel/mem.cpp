#include <cstddef>

#include <asm/port_io.h>
#include <asm/sys.h>
#include <assert.h>
#include <errno.h>
#include <kernel/mem.h>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/task.h>
#include <kernel/vga.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/bitmap.hpp>
#include <types/size.h>
#include <types/status.h>

// constant values

#define EMPTY_PAGE ((page_t)0)

// ---------------------

static size_t mem_size;
static uint8_t _mem_bitmap[1024 * 1024 / 8];
static types::bitmap mem_bitmap(
    [](unsigned char*, std::size_t){}, _mem_bitmap,
    1024 * 1024);

// global
segment_descriptor gdt[7];

uint8_t e820_mem_map[1024];
uint32_t e820_mem_map_count;
uint32_t e820_mem_map_entry_size;
struct mem_size_info mem_size_info;

constexpr void mark_addr_len(pptr_t start, size_t n)
{
    if (n == 0)
        return;
    page_t start_page = align_down<12>(start) >> 12;
    page_t end_page = align_up<12>(start + n) >> 12;
    for (page_t i = start_page; i < end_page; ++i)
        mem_bitmap.set(i);
}

constexpr void free_addr_len(pptr_t start, size_t n)
{
    if (n == 0)
        return;
    page_t start_page = align_down<12>(start) >> 12;
    page_t end_page = align_up<12>(start + n) >> 12;
    for (page_t i = start_page; i < end_page; ++i)
        mem_bitmap.clear(i);
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
    const auto size = mem_bitmap.size();
    for (size_t i = 0; i < size; ++i) {
        if (mem_bitmap.test(i) == 0) {
            mem_bitmap.set(i);
            return i;
        }
    }
    return -1;
}

void __free_raw_page(page_t pg)
{
    mem_bitmap.clear(pg);
}

page allocate_page(void)
{
    return page {
        .phys_page_id = __alloc_raw_page(),
        .ref_count = types::memory::kinew<size_t>(0),
        .pg_pteidx = 0,
        .attr = 0,
    };
}

void free_page(page* pg)
{
    if (*pg->ref_count == 1) {
        types::memory::kidelete<size_t>(pg->ref_count);
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

using kernel::memory::mm_list;
using kernel::memory::mm;

mm_list::mm_list()
    : m_areas(s_kernel_mms->m_areas)
{
    m_pd = __alloc_raw_page();
    kernel::paccess pdst(m_pd), psrc(s_kernel_mms->m_pd);
    auto* dst = pdst.ptr();
    auto* src = psrc.ptr();
    assert(dst && src);
    memcpy(dst, src, PAGE_SIZE);
}

mm_list::mm_list(const mm_list& other)
    : mm_list()
{
    m_brk = other.m_brk;
    for (auto& src : other.m_areas) {
        if (src.is_kernel_space() || src.attr.system)
            continue;

        auto& area = this->addarea(
            src.start, src.attr.write, src.attr.system);

        if (src.attr.mapped) {
            area.attr.mapped = 1;
            area.mapped_file = src.mapped_file;
            area.file_offset = src.file_offset;
        }

        paccess pa(m_pd);
        pd_t pd = (pd_t)pa.ptr();

        for (const auto& pg : *src.pgs) {
            area.append_page(pd, pg,
                    PAGE_COW | (pg.attr & PAGE_MMAP),
                    src.attr.system);
        }
    }
}

mm_list::~mm_list()
{
    if (!m_pd)
        return;

    clear_user();
    dealloc_pd(m_pd);
}

void mm_list::switch_pd() const
{
    asm_switch_pd(m_pd);
}

int mm_list::register_brk(void* addr)
{
    if (!is_avail(addr))
        return GB_FAILED;
    m_brk = &addarea(addr, true, false);
    return GB_OK;
}

void* mm_list::set_brk(void* addr)
{
    assert(m_brk);
    void* curbrk = m_brk->end();

    if (addr <= curbrk || !is_avail(curbrk, vptrdiff(addr, curbrk)))
        return curbrk;

    kernel::paccess pa(m_pd);
    pd_t pd = (pd_t)pa.ptr();

    while (curbrk < addr) {
        m_brk->append_page(pd, empty_page, PAGE_COW, false);
        curbrk = (char*)curbrk + PAGE_SIZE;
    }

    return curbrk;
}

void* mm_list::find_avail(void* hint, size_t len, bool priv) const
{
    void* addr = hint;
    if (!addr) {
        // default value of mmapp'ed area
        if (!priv)
            addr = (void*)0x40000000;
        else
            addr = (void*)0xe0000000;
    }

    while (!is_avail(addr, len)) {
        auto iter = m_areas.lower_bound(addr);
        if (iter == m_areas.end())
            return nullptr;

        addr = iter->end();
    }

    if (!priv && addr >= (void*)0xc0000000)
        return nullptr;

    return addr;
}

// TODO: write dirty pages to file
int mm_list::unmap(void* start, size_t len, bool system)
{
    ptr_t addr = (ptr_t)start;
    void* end = vptradd(start, align_up<12>(len));

    // standard says that addr and len MUST be
    // page-aligned or the call is invalid
    if (addr % PAGE_SIZE != 0)
        return -EINVAL;

    // if doing user mode unmapping, check area privilege
    if (!system) {
        if (addr >= 0xc0000000 || end > (void*)0xc0000000)
            return -EINVAL;
    }

    auto iter = m_areas.lower_bound(start);

    for ( ; iter != m_areas.end() && *iter < end; ) {
        if (!(start < *iter) && start != iter->start) {
            mm newmm = iter->split(start);
            unmap(newmm);
            ++iter;
            continue;
        }
        else if (!(*iter < end)) {
            mm newmm = iter->split(end);
            unmap(*iter);
            m_areas.erase(iter);

            bool inserted;
            std::tie(std::ignore, inserted) = m_areas.emplace(std::move(newmm));
            assert(inserted);
            break;
        }
        else {
            unmap(*iter);
            iter = m_areas.erase(iter);
        }
    }

    return GB_OK;
}

mm& mm_list::add_empty_area(void *start, std::size_t page_count,
    uint32_t page_attr, bool w, bool system)
{
    auto& area = addarea(start, w, system);
    kernel::paccess pa(m_pd);
    pd_t pd = (pd_t)pa.ptr();

    while (page_count--)
        area.append_page(pd, empty_page, page_attr, system);

    return area;
}

constexpr void map_raw_page_to_pte(
    pte_t* pte, page_t page,
    bool present, bool write, bool priv)
{
    // set P bit
    pte->v = 0;
    pte->in.p = present;
    pte->in.rw = write;
    pte->in.us = !priv;
    pte->in.page = page;
}

void mm::append_page(pd_t pd, const page& pg, uint32_t attr, bool priv)
{
    assert(pd);

    void* addr = this->end();
    pde_t* pde = *pd + v_to_pdi(addr);

    page_t pt_pg = 0;
    pte_t* pte = nullptr;
    // page table not exist
    if (!pde->in.p) [[unlikely]] {
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

    this->pgs->emplace_back(pg);
    auto& emplaced = this->pgs->back();
    emplaced.pg_pteidx = (pt_pg << 12) + pti;
    emplaced.attr = attr;
}

mm mm::split(void *addr)
{
    assert(addr > start && addr < end());
    assert((ptr_t)addr % PAGE_SIZE == 0);

    size_t this_count = vptrdiff(addr, start) / PAGE_SIZE;
    size_t new_count = pgs->size() - this_count;

    mm newmm {
        .start = addr,
        .attr { attr },
        .pgs = types::memory::kinew<mm::pages_vector>(),
        .mapped_file = mapped_file,
        .file_offset = attr.mapped ? file_offset + this_count * PAGE_SIZE : 0,
    };

    for (size_t i = 0; i < new_count; ++i) {
        newmm.pgs->emplace_back(pgs->back());
        pgs->pop_back();
    }

    return newmm;
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

    if (file && !S_ISREG(file->mode) && !S_ISBLK(file->mode)) [[unlikely]] {
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

    if (file) {
        auto& mm = mms.add_empty_area(hint, n_pgs, PAGE_MMAP | PAGE_COW, write, priv);

        mm.attr.mapped = 1;
        mm.mapped_file = file;
        mm.file_offset = offset;
    }
    else {
        // private mapping of zero-filled pages
        auto& mm = mms.add_empty_area(hint, n_pgs, PAGE_COW, write, priv);

        mm.attr.mapped = 0;
    }

    return GB_OK;
}

SECTION(".text.kinit")
void init_mem(void)
{
    init_mem_layout();

    // TODO: replace early kernel pd
    auto* __kernel_mms = types::memory::kinew<kernel::memory::mm_list>(EARLY_KERNEL_PD_PAGE);
    kernel::memory::mm_list::s_kernel_mms = __kernel_mms;

    // create empty_page struct
    empty_page.attr = 0;
    empty_page.phys_page_id = EMPTY_PAGE;
    empty_page.ref_count = types::memory::kinew<size_t>(2);
    empty_page.pg_pteidx = 0x00002000;

    // 0xd0000000 to 0xd4000000 or 3.5GiB, size 64MiB
    __kernel_mms->add_empty_area(KERNEL_HEAP_START,
        64 * 1024 * 1024 / PAGE_SIZE, PAGE_COW, true, true);

    kernel::kinit::init_kernel_heap(KERNEL_HEAP_START,
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
    types::memory::ident_allocator<std::pair<page_t, mapped_area>>>
    mapped;
static uint8_t _freebm[0x400 / 8];
static types::bitmap freebm(
    [](unsigned char*, std::size_t){}, _freebm, 0x400);
} // namespace __physmapper

void* kernel::pmap(page_t pg, bool cached)
{
    auto* const pmap_pt = std::bit_cast<pte_t*>(0xff001000);
    auto* const mapped_start = std::bit_cast<void*>(0xff000000);

    auto iter = __physmapper::mapped.find(pg);
    if (iter) {
        auto [ idx, area ] = *iter;
        ++area.ref;
        return area.ptr;
    }

    for (int i = 2; i < 0x400; ++i) {
        if (__physmapper::freebm.test(i) == 0) {
            auto* pte = pmap_pt + i;
            if (cached)
                pte->v = 0x3;
            else
                pte->v = 0x13;
            pte->in.page = pg;

            void* ptr = vptradd(mapped_start, 0x1000 * i);
            invalidate_tlb(ptr);

            __physmapper::freebm.set(i);
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
    auto& [ ref, ptr ] = iter->second;

    if (ref > 1) {
        --ref;
        return;
    }

    int i = vptrdiff(ptr, mapped_start);
    i /= 0x1000;

    auto* pte = pmap_pt + i;
    pte->v = 0;
    invalidate_tlb(ptr);

    __physmapper::freebm.clear(i);
    __physmapper::mapped.remove(iter);
}
