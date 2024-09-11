#include <assert.h>
#include <stdint.h>

#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/vm_area.hpp>

using namespace kernel::mem;

static inline void __invalidate_all_tlb() {
    asm volatile(
        "mov %%cr3, %%rax\n\t"
        "mov %%rax, %%cr3\n\t"
        :
        :
        : "rax", "memory");
}

static inline void __dealloc_page_table_all(paging::pfn_t pt, int depth,
                                            int from, int to) {
    using namespace paging;

    if (depth > 1) {
        for (int i = from; i < to; ++i) {
            auto pse = PSE{pt}[i];
            if (!(pse.attributes() & PA_P))
                continue;

            int pfn = pse.pfn();
            __dealloc_page_table_all(pfn, depth - 1, 0, 512);
        }
    }

    free_page(pt);
}

static inline void __dealloc_page_table(paging::pfn_t pt) {
    using namespace paging;
    auto start_idx = idx_p4(0);
    auto end_idx = idx_p4(KERNEL_SPACE_START);

    __dealloc_page_table_all(pt, 4, start_idx, end_idx);
}

mm_list::mm_list() : m_pt{paging::alloc_page_table()}, m_brk{m_areas.end()} {
    memcpy(physaddr<void>{m_pt}, paging::KERNEL_PAGE_TABLE_PHYS_ADDR, 0x1000);
}

mm_list::mm_list(const mm_list& other) : mm_list{} {
    m_areas = other.m_areas;

    using namespace paging;
    for (auto iter = m_areas.begin(); iter != m_areas.end(); ++iter) {
        auto& area = *iter;

        if (area.flags & MM_BREAK)
            m_brk = iter;

        auto this_iter = vaddr_range{m_pt, area.start, area.end};
        auto other_iter = vaddr_range{other.m_pt, area.start, area.end};

        while (this_iter) {
            auto this_pte = *this_iter, other_pte = *other_iter;
            auto attributes = other_pte.attributes();
            auto pfn = other_pte.pfn();

            attributes &= ~(PA_RW | PA_A | PA_D);
            attributes |= PA_COW;
            this_pte.set(attributes, pfn);

            increase_refcount(pfn_to_page(pfn));

            // TODO: create a function to set COW mappings
            attributes = other_pte.attributes();
            attributes &= ~PA_RW;
            attributes |= PA_COW;
            other_pte.set(attributes, pfn);

            ++this_iter, ++other_iter;
        }
    }

    __invalidate_all_tlb();
}

mm_list::~mm_list() {
    if (!m_pt)
        return;

    clear();
    __dealloc_page_table(m_pt);
}

bool mm_list::is_avail(uintptr_t start, std::size_t len) const noexcept {
    start &= ~0xfff;
    uintptr_t end = (start + len + 0xfff) & ~0xfff;
    len = end - start;

    if (end > USER_SPACE_MEMORY_TOP)
        return false;

    for (const auto& area : m_areas) {
        if (!area.is_avail(start, end))
            return false;
    }
    return true;
}

bool mm_list::is_avail(uintptr_t addr) const {
    if (addr >= USER_SPACE_MEMORY_TOP)
        return false;

    auto iter = m_areas.find(addr);
    return iter == m_areas.end();
}

uintptr_t mm_list::find_avail(uintptr_t hint, size_t len) const {
    auto addr = std::max(hint, MMAP_MIN_ADDR);

    while (!is_avail(addr, len)) {
        auto iter = m_areas.lower_bound(addr);
        if (iter == m_areas.end())
            return 0;

        addr = iter->end;
    }

    return addr;
}

void mm_list::switch_pd() const noexcept {
    asm volatile("mov %0, %%cr3" : : "r"(m_pt) : "memory");
}

int mm_list::register_brk(uintptr_t addr) {
    assert(m_brk == m_areas.end());
    if (!is_avail(addr))
        return -ENOMEM;

    bool inserted;
    std::tie(m_brk, inserted) =
        m_areas.emplace(addr, MM_ANONYMOUS | MM_WRITE | MM_BREAK);

    assert(inserted);
    return 0;
}

uintptr_t mm_list::set_brk(uintptr_t addr) {
    using namespace paging;
    assert(m_brk != m_areas.end());
    uintptr_t curbrk = m_brk->end;

    addr += 4096 - 1;
    addr &= ~0xfff;

    if (addr <= curbrk || !is_avail(curbrk, addr - curbrk))
        return curbrk;

    for (auto pte : vaddr_range{m_pt, curbrk, addr})
        pte.set(PA_ANONYMOUS_PAGE | PA_NXE, EMPTY_PAGE_PFN);

    m_brk->end = addr;
    return m_brk->end;
}

void mm_list::clear() {
    for (auto iter = m_areas.begin(); iter != m_areas.end(); ++iter)
        unmap(iter, false);

    __invalidate_all_tlb();

    m_areas.clear();
    m_brk = m_areas.end();
}

mm_list::iterator mm_list::split(iterator area, uintptr_t addr) {
    assert(!(addr & 0xfff));
    assert(addr > area->start && addr < area->end);

    std::size_t old_len = addr - area->start;
    std::size_t new_file_offset = 0;

    if (area->mapped_file)
        new_file_offset = area->file_offset + old_len;

    auto new_end = area->end;
    area->end = addr;

    auto [iter, inserted] = m_areas.emplace(addr, area->flags, new_end,
                                            area->mapped_file, new_file_offset);

    assert(inserted);
    return iter;
}

int mm_list::unmap(iterator area, bool should_invalidate_tlb) {
    using namespace paging;

    bool should_use_invlpg = area->end - area->start <= 0x4000;
    auto range = vaddr_range{m_pt, area->start, area->end};
    uintptr_t cur_addr = area->start;

    // TODO: write back dirty pages
    for (auto pte : range) {
        free_page(pte.pfn());
        pte.clear();

        if (should_invalidate_tlb && should_use_invlpg) {
            asm volatile("invlpg (%0)" : : "r"(cur_addr) : "memory");
            cur_addr += 0x1000;
        }
    }

    if (should_invalidate_tlb && !should_use_invlpg)
        __invalidate_all_tlb();

    return 0;
}

int mm_list::unmap(uintptr_t start, std::size_t length,
                   bool should_invalidate_tlb) {
    // standard says that addr and len MUST be
    // page-aligned or the call is invalid
    if (start & 0xfff)
        return -EINVAL;

    uintptr_t end = (start + length + 0xfff) & ~0xfff;

    // check address validity
    if (end > KERNEL_SPACE_START)
        return -EINVAL;
    if (end > USER_SPACE_MEMORY_TOP)
        return -ENOMEM;

    auto iter = m_areas.lower_bound(start);
    auto iter_end = m_areas.upper_bound(end);

    // start <= iter <= end a.k.a. !(start > *iter) && !(*iter > end)
    while (iter != iter_end) {
        // start == iter:
        // start is between (iter->start, iter->end)
        //
        // strip out the area before start
        if (!(start < *iter) && start != iter->start)
            iter = split(iter, start);

        // iter.end <= end
        // it is safe to unmap the area directly
        if (*iter < end) {
            if (int ret = unmap(iter, should_invalidate_tlb); ret != 0)
                return ret;

            iter = m_areas.erase(iter);
            continue;
        }

        // end == iter:
        // end is between [iter->start, iter->end)
        //
        // if end == iter->start, no need to strip the area
        if (end == iter->start) {
            ++iter;
            continue;
        }

        (void)split(iter, end);
        if (int ret = unmap(iter, should_invalidate_tlb); ret != 0)
            return ret;

        iter = m_areas.erase(iter);

        // no need to check areas after this
        break;
    }

    return 0;
}

int mm_list::mmap(const map_args& args) {
    auto& vaddr = args.vaddr;
    auto& length = args.length;
    auto& finode = args.file_inode;
    auto& foff = args.file_offset;
    auto& flags = args.flags;

    assert((vaddr & 0xfff) == 0 && (foff & 0xfff) == 0);
    assert((length & 0xfff) == 0 && length != 0);

    if (!is_avail(vaddr, length))
        return -EEXIST;

    using namespace kernel::mem::paging;

    // PA_RW is set during page fault while PA_NXE is preserved
    // so we set PA_NXE now
    psattr_t attributes = PA_US;
    if (!(flags & MM_EXECUTE))
        attributes |= PA_NXE;

    if (flags & MM_MAPPED) {
        assert(finode);
        assert(S_ISREG(finode->mode) || S_ISBLK(finode->mode));

        auto [area, inserted] = m_areas.emplace(
            vaddr, flags & ~MM_INTERNAL_MASK, vaddr + length, finode, foff);
        assert(inserted);

        attributes |= PA_MMAPPED_PAGE;
        for (auto pte : vaddr_range{m_pt, vaddr, vaddr + length})
            pte.set(attributes, EMPTY_PAGE_PFN);
    } else if (flags & MM_ANONYMOUS) {
        // private mapping of zero-filled pages
        // TODO: shared mapping
        auto [area, inserted] =
            m_areas.emplace(vaddr, (flags & ~MM_INTERNAL_MASK), vaddr + length);
        assert(inserted);

        attributes |= PA_ANONYMOUS_PAGE;
        for (auto pte : vaddr_range{m_pt, vaddr, vaddr + length})
            pte.set(attributes, EMPTY_PAGE_PFN);
    } else {
        return -EINVAL;
    }

    return 0;
}
