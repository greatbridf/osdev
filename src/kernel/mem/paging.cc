#include <assert.h>
#include <string.h>

#include <types/list.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/slab.hpp>
#include <kernel/process.hpp>

using namespace types::list;

using namespace kernel::async;
using namespace kernel::mem::paging;

static struct zone_info {
    page* next;
    std::size_t count;
} zones[52];

static mutex zone_lock;

constexpr unsigned _msb(std::size_t x) {
    unsigned n = 0;
    while (x >>= 1)
        n++;
    return n;
}

constexpr pfn_t buddy(pfn_t pfn, unsigned order) {
    return pfn ^ (1 << (order + 12));
}

constexpr pfn_t parent(pfn_t pfn, unsigned order) {
    return pfn & ~(1 << (order + 12));
}

// call with zone_lock held
static inline void _zone_list_insert(unsigned order, page* zone) {
    assert(zone->flags & PAGE_PRESENT && zone->flags & PAGE_BUDDY);
    assert((zone->flags & 0xff) == 0);
    zone->flags |= order;

    zones[order].count++;
    list_insert(&zones[order].next, zone);
}

// call with zone_lock held
static inline void _zone_list_remove(unsigned order, page* zone) {
    assert(zone->flags & PAGE_PRESENT && zone->flags & PAGE_BUDDY);
    assert(zones[order].count > 0 && (zone->flags & 0xff) == order);
    zone->flags &= ~0xff;

    zones[order].count--;
    list_remove(&zones[order].next, zone);
}

// call with zone_lock held
static inline page* _zone_list_get(unsigned order) {
    if (zones[order].count == 0)
        return nullptr;

    zones[order].count--;
    auto* pg = list_get(&zones[order].next);

    assert((pg->flags & 0xff) == order);
    return pg;
}

// where order represents power of 2
// call with zone_lock held
static inline page* _create_zone(pfn_t pfn, unsigned order) {
    page* zone = pfn_to_page(pfn);

    assert(zone->flags & PAGE_PRESENT);
    zone->flags |= PAGE_BUDDY;

    _zone_list_insert(order, zone);
    return zone;
}

// call with zone_lock held
static inline void _split_zone(page* zone, unsigned order, unsigned target_order) {
    while (order > target_order) {
        pfn_t pfn = page_to_pfn(zone);
        _create_zone(buddy(pfn, order - 1), order - 1);

        order--;
    }

    zone->flags &= ~0xff;
    zone->flags |= target_order;
}

// call with zone_lock held
static inline page* _alloc_zone(unsigned order) {
    for (unsigned i = order; i < 52; ++i) {
        auto zone = _zone_list_get(i);
        if (!zone)
            continue;

        zone->refcount++;

        if (i > order)
            _split_zone(zone, i, order);

        assert(zone->flags & PAGE_PRESENT && zone->flags & PAGE_BUDDY);
        return zone;
    }

    return nullptr;
}

constexpr uintptr_t _find_mid(uintptr_t l, uintptr_t r) {
    if (l == r)
        return l;
    uintptr_t bit = 1 << _msb(l ^ r);

    return (l & r & ~(bit - 1)) | bit;
}

static void _recur_create_zone(uintptr_t l, uintptr_t r) {
    auto mid = _find_mid(l, r);
    assert(l <= mid);

    // empty zone
    if (l == mid) {
        assert(l == r);
        return;
    }

    // create [l, r) directly
    if (r == mid) {
        auto diff = r - l;
        int order = 0;
        while ((1u << order) <= diff) {
            while (!(diff & (1 << order)))
                order++;
            _create_zone(l << 12, order);

            l += (1 << order);
            diff &= ~(1 << order);
        }

        return;
    }

    // split into halves
    _recur_create_zone(l, mid);
    _recur_create_zone(mid, r);
}

void kernel::mem::paging::create_zone(uintptr_t start, uintptr_t end) {
    start += (4096 - 1);
    start >>= 12;
    end >>= 12;

    if (start >= end)
        return;

    lock_guard_irq lock{zone_lock};

    _recur_create_zone(start, end);
}

void kernel::mem::paging::mark_present(uintptr_t start, uintptr_t end) {
    start >>= 12;

    end += (4096 - 1);
    end >>= 12;

    while (start < end)
        PAGE_ARRAY[start++].flags |= PAGE_PRESENT;
}

page* kernel::mem::paging::alloc_pages(unsigned order) {
    lock_guard_irq lock{zone_lock};
    auto* zone = _alloc_zone(order);
    if (!zone)
        freeze();

    return zone;
}

page* kernel::mem::paging::alloc_page() {
    return alloc_pages(0);
}

pfn_t kernel::mem::paging::alloc_page_table() {
    page* zone = alloc_page();
    pfn_t pfn = page_to_pfn(zone);

    memset(physaddr<void>{pfn}, 0x00, 0x1000);

    return pfn;
}

void kernel::mem::paging::free_pages(page* pg, unsigned order) {
    lock_guard_irq lock{zone_lock};
    assert((pg->flags & 0xff) == order);

    if (!(pg->flags & PAGE_BUDDY) || --pg->refcount)
        return;

    while (order < 52) {
        pfn_t pfn = page_to_pfn(pg);
        pfn_t buddy_pfn = buddy(pfn, order);
        page* buddy_page = pfn_to_page(buddy_pfn);

        if (!(buddy_page->flags & PAGE_BUDDY))
            break;

        if ((buddy_page->flags & 0xff) != order)
            break;

        if (buddy_page->refcount)
            break;

        _zone_list_remove(order, buddy_page);

        if (buddy_page < pg)
            std::swap(buddy_page, pg);

        buddy_page->flags &= ~(PAGE_BUDDY | 0xff);
        order++;
    }

    pg->flags &= ~0xff;
    _zone_list_insert(order, pg);
}

void kernel::mem::paging::free_page(page* page) {
    return free_pages(page, 0);
}

void kernel::mem::paging::free_pages(pfn_t pfn, unsigned order) {
    return free_pages(pfn_to_page(pfn), order);
}

void kernel::mem::paging::free_page(pfn_t pfn) {
    return free_page(pfn_to_page(pfn));
}

pfn_t kernel::mem::paging::page_to_pfn(page* _page) {
    return (pfn_t)(_page - PAGE_ARRAY) * 0x1000;
}

page* kernel::mem::paging::pfn_to_page(pfn_t pfn) {
    return PAGE_ARRAY + pfn / 0x1000;
}

void kernel::mem::paging::increase_refcount(page* pg) {
    lock_guard_irq lock{zone_lock};
    pg->refcount++;
}
