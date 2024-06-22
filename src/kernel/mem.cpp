#include <cstddef>

#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>

#include <kernel/mem/paging.hpp>
#include <kernel/mm.hpp>
#include <kernel/process.hpp>
#include <kernel/task.h>
#include <kernel/vga.hpp>

#include <types/allocator.hpp>

void dealloc_pd(kernel::mem::paging::pfn_t pd)
{
    // TODO: LONG MODE
    // {
    //     kernel::paccess pa(pd);
    //     auto p_pd = (pd_t)pa.ptr();
    //     assert(p_pd);
    //     for (pde_t* ent = (*p_pd); ent < (*p_pd) + 768; ++ent) {
    //         if (!ent->in.p)
    //             continue;
    //         __free_raw_page(ent->in.pt_page);
    //     }
    // }
    // __free_raw_page(pd);
}

using kernel::mem::mm_list;
using kernel::mem::mm;

mm_list::mm_list()
    : m_areas(s_kernel_mms->m_areas)
{
    // TODO: LONG MODE
    // m_pd = __alloc_raw_page();
    // kernel::paccess pdst(m_pd), psrc(s_kernel_mms->m_pd);
    // auto* dst = pdst.ptr();
    // auto* src = psrc.ptr();
    // assert(dst && src);
    // memcpy(dst, src, PAGE_SIZE);
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

        // TODO: LONG MODE
        // paccess pa(m_pd);
        // pd_t pd = (pd_t)pa.ptr();

        // for (const auto& pg : *src.pgs) {
        //     area.append_page(pd, pg,
        //             PAGE_COW | (pg.attr & PAGE_MMAP),
        //             src.attr.system);
        // }
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
    // TODO: LONG MODE
    // asm_switch_pd(m_pd);
}

int mm_list::register_brk(void* addr)
{
    if (!is_avail(addr))
        return -ENOMEM;
    m_brk = &addarea(addr, true, false);
    return 0;
}

void* mm_list::set_brk(void* addr)
{
    assert(m_brk);
    void* curbrk = m_brk->end();

    if (addr <= curbrk || !is_avail(curbrk, vptrdiff(addr, curbrk)))
        return curbrk;

    // TODO: LONG MODE
    // kernel::paccess pa(m_pd);
    // pd_t pd = (pd_t)pa.ptr();

    // while (curbrk < addr) {
    //     m_brk->append_page(pd, empty_page, PAGE_COW, false);
    //     curbrk = (char*)curbrk + PAGE_SIZE;
    // }

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
    uintptr_t addr = (uintptr_t)start;
    void* end = vptradd(start, align_up<12>(len));

    // standard says that addr and len MUST be
    // page-aligned or the call is invalid
    if ((addr & 0xfff) != 0)
        return -EINVAL;

    // if doing user mode unmapping, check area privilege
    if (!system) {
        if (addr >= 0xc0000000 || end > (void*)0xc0000000)
            return -EINVAL;
    }

    auto iter = m_areas.lower_bound(start);

    // TODO: LONG MODE
    for ( ; iter != m_areas.end() && *iter < end; ) {
        if (!(start < *iter) && start != iter->start) {
            mm newmm = iter->split(start);
            // unmap(newmm);
            ++iter;
            continue;
        }
        else if (!(*iter < end)) {
            mm newmm = iter->split(end);
            // unmap(*iter);
            m_areas.erase(iter);

            bool inserted;
            std::tie(std::ignore, inserted) = m_areas.emplace(std::move(newmm));
            assert(inserted);
            break;
        }
        else {
            // unmap(*iter);
            iter = m_areas.erase(iter);
        }
    }

    return 0;
}

mm& mm_list::add_empty_area(void *start, std::size_t page_count,
    uint32_t page_attr, bool w, bool system)
{
    // TODO: LONG MODE
    // auto& area = addarea(start, w, system);
    // kernel::paccess pa(m_pd);
    // pd_t pd = (pd_t)pa.ptr();

    // while (page_count--)
    //     area.append_page(pd, empty_page, page_attr, system);

    // return area;
}

// TODO: LONG MODE
// constexpr void map_raw_page_to_pte(
//     pte_t* pte, kernel::mem::paging::pfn_t page,
//     bool present, bool write, bool priv)
// {
//     // set P bit
//     pte->v = 0;
//     pte->in.p = present;
//     pte->in.rw = write;
//     pte->in.us = !priv;
//     pte->in.page = page;
// }

// TODO: LONG MODE
// void mm::append_page(pd_t pd, const page& pg, uint32_t attr, bool priv)
// {
//     assert(pd);
// 
//     void* addr = this->end();
//     pde_t* pde = *pd + v_to_pdi(addr);
// 
//     kernel::mem::paging::pfn_t pt_pg = 0;
//     pte_t* pte = nullptr;
//     // page table not exist
//     if (!pde->in.p) [[unlikely]] {
//         // allocate a page for the page table
//         pt_pg = __alloc_raw_page();
//         pde->in.p = 1;
//         pde->in.rw = 1;
//         pde->in.us = 1;
//         pde->in.pt_page = pt_pg;
// 
//         auto pt = (pt_t)kernel::pmap(pt_pg);
//         assert(pt);
//         pte = *pt;
// 
//         memset(pt, 0x00, PAGE_SIZE);
//     } else {
//         pt_pg = pde->in.pt_page;
//         auto pt = (pt_t)kernel::pmap(pt_pg);
//         assert(pt);
//         pte = *pt;
//     }
// 
//     // map the page in the page table
//     int pti = v_to_pti(addr);
//     pte += pti;
// 
//     map_raw_kernel::mem::paging::pfn_to_pte(
//         pte,
//         pg.phys_page_id,
//         !(attr & PAGE_MMAP),
//         false,
//         priv);
// 
//     kernel::pfree(pt_pg);
// 
//     if (unlikely((attr & PAGE_COW) && !(pg.attr & PAGE_COW))) {
//         kernel::paccess pa(pg.pg_pteidx >> 12);
//         auto* pg_pte = (pte_t*)pa.ptr();
//         assert(pg_pte);
//         pg_pte += (pg.pg_pteidx & 0xfff);
//         pg.attr |= PAGE_COW;
//         pg_pte->in.rw = 0;
//         pg_pte->in.a = 0;
//         invalidate_tlb(addr);
//     }
// 
//     ++*pg.ref_count;
// 
//     this->pgs->emplace_back(pg);
//     auto& emplaced = this->pgs->back();
//     emplaced.pg_pteidx = (pt_pg << 12) + pti;
//     emplaced.attr = attr;
// }

mm mm::split(void *addr)
{
    assert(addr > start && addr < end());
    assert((uintptr_t)addr % 4096 == 0);

    size_t this_count = vptrdiff(addr, start) / 4096;
    size_t new_count = page_count - this_count;

    mm newmm {
        .start = addr,
        .attr { attr },
        .mapped_file = mapped_file,
        .file_offset = attr.mapped ? file_offset + this_count * 4096 : 0,
        .page_count = 0,
    };

    // TODO:
    // for (size_t i = 0; i < new_count; ++i) {
    //     newmm.pgs->emplace_back(pgs->back());
    //     pgs->pop_back();
    // }

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

    if (file && !S_ISREG(file->mode) && !S_ISBLK(file->mode)) [[unlikely]]
        return -EINVAL;

    // TODO: find another address
    assert(((uintptr_t)hint & 0xfff) == 0);
    // TODO: return failed
    assert((offset & 0xfff) == 0);

    size_t n_pgs = align_up<12>(len) >> 12;

    if (!mms.is_avail(hint, len))
        return -EEXIST;

    // TODO: LONG MODE
    using namespace kernel::mem::paging;

    if (file) {
        auto& mm = mms.add_empty_area(hint, n_pgs, PA_MMAP | PA_COW, write, priv);

        mm.attr.mapped = 1;
        mm.mapped_file = file;
        mm.file_offset = offset;
    }
    else {
        // private mapping of zero-filled pages
        auto& mm = mms.add_empty_area(hint, n_pgs, PA_COW, write, priv);

        mm.attr.mapped = 0;
    }

    return 0;
}
