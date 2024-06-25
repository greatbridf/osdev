#include <assert.h>
#include <string.h>

#include <types/list.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/slab.hpp>
#include <kernel/mem/vm_area.hpp>
#include <kernel/process.hpp>

using namespace types::list;

using namespace kernel::async;
using namespace kernel::mem::paging;

static inline void __page_fault_die(uintptr_t vaddr)
{
    kmsgf("[kernel] kernel panic: invalid memory access to %p", vaddr);
    freeze();
}

static inline PSE __parse_pse(PSE pse, bool priv)
{
    auto attr = priv ? PA_KERNEL_PAGE_TABLE : PA_PAGE_TABLE;
    if (!(pse.attributes() & PA_P))
        pse.set(attr, alloc_page_table());

    return pse.parse();
}

static struct zone_info {
    page* next;
    std::size_t count;
} zones[52];

static mutex zone_lock;

constexpr int _msb(std::size_t x)
{
    int n = 0;
    while (x >>= 1)
        n++;
    return n;
}

constexpr pfn_t buddy(pfn_t pfn, int order)
{
    return pfn ^ (1 << (order + 12));
}

constexpr pfn_t parent(pfn_t pfn, int order)
{
    return pfn & ~(1 << (order + 12));
}

// call with zone_lock held
static inline void _zone_list_insert(int order, page* zone)
{
    zones[order].count++;
    list_insert(&zones[order].next, zone);
}

// call with zone_lock held
static inline void _zone_list_remove(int order, page* zone)
{
    zones[order].count--;
    list_remove(&zones[order].next, zone);
}

// call with zone_lock held
static inline page* _zone_list_get(int order)
{
    if (zones[order].count == 0)
        return nullptr;

    zones[order].count--;
    return list_get(&zones[order].next);
}

// where order represents power of 2
// call with zone_lock held
static inline page* _create_zone(pfn_t pfn, int order)
{
    page* zone = pfn_to_page(pfn);

    assert(zone->flags & PAGE_PRESENT);
    zone->flags |= PAGE_BUDDY;

    _zone_list_insert(order, zone);
    return zone;
}

// call with zone_lock held
static inline void _split_zone(page* zone, int order, int target_order)
{
    while (order > target_order) {
        pfn_t pfn = page_to_pfn(zone);
        _create_zone(buddy(pfn, order - 1), order - 1);

        order--;
    }
}

// call with zone_lock held
static inline page* _alloc_zone(int order)
{
    for (int i = order; i < 52; ++i) {
        auto zone = _zone_list_get(i);
        if (!zone)
            continue;

        increase_refcount(zone);

        if (i > order)
            _split_zone(zone, i, order);

        assert(zone->flags & PAGE_PRESENT && zone->flags & PAGE_BUDDY);
        return zone;
    }

    return nullptr;
}

void kernel::mem::paging::create_zone(uintptr_t start, uintptr_t end)
{
    start += (4096 - 1);
    start >>= 12;
    end >>= 12;

    if (start >= end)
        return;

    lock_guard_irq lock{zone_lock};

    unsigned long low = start;
    for (int i = 0; i < _msb(end); ++i, low >>= 1) {
        if (!(low & 1))
            continue;
        _create_zone(low << (12+i), i);
        low++;
    }

    low = 1 << _msb(end);
    while (low < end) {
        int order = _msb(end - low);
        _create_zone(low << 12, order);
        low |= (1 << order);
    }
}

void kernel::mem::paging::mark_present(uintptr_t start, uintptr_t end)
{
    start >>= 12;

    end += (4096 - 1);
    end >>= 12;

    while (start < end)
        PAGE_ARRAY[start++].flags |= PAGE_PRESENT;
}

page* kernel::mem::paging::alloc_pages(int order)
{
    lock_guard_irq lock{zone_lock};
    auto* zone = _alloc_zone(order);
    if (!zone)
        freeze();

    return zone;
}

page* kernel::mem::paging::alloc_page()
{
    return alloc_pages(0);
}

pfn_t kernel::mem::paging::alloc_page_table()
{
    page* zone = alloc_page();
    pfn_t pfn = page_to_pfn(zone);

    memset(physaddr<void>{pfn}, 0x00, 0x1000);

    return pfn;
}

void kernel::mem::paging::free_pages(page* pg, int order)
{
    // TODO: atomic
    if (!(pg->flags & PAGE_BUDDY) || --pg->refcount)
        return;

    lock_guard_irq lock{zone_lock};
    while (order < 52) {
        pfn_t pfn = page_to_pfn(pg);
        pfn_t buddy_pfn = buddy(pfn, order);
        page* buddy_page = pfn_to_page(buddy_pfn);

        if (!(buddy_page->flags & PAGE_BUDDY) || buddy_page->refcount)
            break;

        _zone_list_remove(order, buddy_page);

        if (buddy_page < pg)
            std::swap(buddy_page, pg);

        buddy_page->flags &= ~PAGE_BUDDY;
        order++;
    }

    _zone_list_insert(order, pg);
}

void kernel::mem::paging::free_page(page* page)
{
    return free_pages(page, 0);
}

void kernel::mem::paging::free_pages(pfn_t pfn, int order)
{
    return free_pages(pfn_to_page(pfn), order);
}

void kernel::mem::paging::free_page(pfn_t pfn)
{
    return free_page(pfn_to_page(pfn));
}

pfn_t kernel::mem::paging::page_to_pfn(page* _page)
{
    return (pfn_t)(_page - PAGE_ARRAY) * 0x1000;
}

page* kernel::mem::paging::pfn_to_page(pfn_t pfn)
{
    return PAGE_ARRAY + pfn / 0x1000;
}

void kernel::mem::paging::increase_refcount(page* pg)
{
    pg->refcount++;
}

void kernel::mem::paging::handle_page_fault(unsigned long err)
{
    using namespace kernel::mem;
    using namespace paging;

    uintptr_t vaddr;
    asm volatile("mov %%cr2, %0": "=g"(vaddr): : );
    auto& mms = current_process->mms;

    auto* mm_area = mms.find(vaddr);
    if (!mm_area) [[unlikely]] {
        // user access of address that does not exist
        if (err & PAGE_FAULT_U)
            kill_current(SIGSEGV);

        __page_fault_die(vaddr);
    }

    if (err & PAGE_FAULT_U && err & PAGE_FAULT_P)
        kill_current(SIGSEGV);

    auto idx = idx_all(vaddr);

    auto pe = mms.get_page_table()[std::get<1>(idx)];
    assert(pe.attributes() & PA_P);
    pe = pe.parse()[std::get<2>(idx)];
    assert(pe.attributes() & PA_P);
    pe = pe.parse()[std::get<3>(idx)];
    assert(pe.attributes() & PA_P);
    pe = pe.parse()[std::get<4>(idx)];

    bool mmapped = mm_area->flags & MM_MAPPED;
    assert(!mmapped || mm_area->mapped_file);

    if (!(err & PAGE_FAULT_P) && !mmapped) [[unlikely]]
        __page_fault_die(vaddr);

    pfn_t pfn = pe.pfn();
    auto attr = pe.attributes();

    page* pg = pfn_to_page(pfn);

    if (attr & PA_COW) {
        attr &= ~PA_COW;
        if (mm_area->flags & MM_WRITE)
            attr |= PA_RW;
        else
            attr &= ~PA_RW;

        // if it is a dying page
        // TODO: use atomic
        if (pg->refcount == 1) {
            pe.set(attr, pfn);
            return;
        }

        // duplicate the page
        page* new_page = alloc_page();
        pfn_t new_pfn = page_to_pfn(new_page);
        physaddr<void> new_page_addr{new_pfn};

        if (attr & PA_ANON)
            memset(new_page_addr, 0x00, 0x1000);
        else
            memcpy(new_page_addr, physaddr<void>{pfn}, 0x1000);

        attr &= ~(PA_A | PA_ANON);
        --pg->refcount;

        pe.set(attr, new_pfn);
        pfn = new_pfn;
    }

    if (attr & PA_MMAP) {
        attr |= PA_P;

        size_t offset = (vaddr & ~0xfff) - mm_area->start;
        char* data = physaddr<char>{pfn};

        int n = vfs_read(
            mm_area->mapped_file,
            data,
            4096,
            mm_area->file_offset + offset,
            4096);

        // TODO: send SIGBUS if offset is greater than real size
        if (n != 4096)
            memset(data + n, 0x00, 4096 - n);

        // TODO: shared mapping
        attr &= ~PA_MMAP;

        pe.set(attr, pfn);
    }
}

vaddr_range::vaddr_range(pfn_t pt, uintptr_t start, uintptr_t end, bool priv)
    : n {start >= end ? 0 : ((end - start) >> 12)}
    , idx4{!n ? 0 : idx_p4(start)}
    , idx3{!n ? 0 : idx_p3(start)}
    , idx2{!n ? 0 : idx_p2(start)}
    , idx1{!n ? 0 : idx_p1(start)}
    , pml4{!n ? PSE{0} : PSE{pt}}
    , pdpt{!n ? PSE{0} : __parse_pse(pml4[idx4], priv)}
    , pd{!n ? PSE{0} : __parse_pse(pdpt[idx3], priv)}
    , pt{!n ? PSE{0} : __parse_pse(pd[idx2], priv)}
    , m_start{!n ? 0 : start}, m_end{!n ? 0 : end}
    , is_privilege{!n ? false : priv} { }

vaddr_range::vaddr_range(std::nullptr_t)
    : n{}
    , idx4{}, idx3{}, idx2{}, idx1{}
    , pml4{0}, pdpt{0}
    , pd{0}, pt{0}
    , m_start{}, m_end{}, is_privilege{} { }

vaddr_range vaddr_range::begin() const noexcept
{
    return *this;
}

vaddr_range vaddr_range::end() const noexcept
{
    return vaddr_range {nullptr};
}

PSE vaddr_range::operator*() const noexcept
{
    return pt[idx1];
}

vaddr_range& vaddr_range::operator++()
{
    --n;

    if ((idx1 = (idx1+1)%512) != 0)
        return *this;

    do {
        if ((idx2 = (idx2+1)%512) != 0)
            break;
        do {
            if ((idx3 = (idx3+1)%512) != 0)
                break;

            idx4 = (idx4+1) % 512;

            // if idx4 is 0 after update, we have an overflow
            assert(idx4 != 0);

            pdpt = __parse_pse(pml4[idx4], is_privilege);
        } while (false);

        pd = __parse_pse(pdpt[idx3], is_privilege);
    } while (false);

    pt = __parse_pse(pd[idx2], is_privilege);
    return *this;
}

vaddr_range::operator bool() const noexcept
{
    return n;
}

bool vaddr_range::operator==(const vaddr_range& other) const noexcept
{
    return n == other.n;
}
