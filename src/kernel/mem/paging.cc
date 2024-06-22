#include <assert.h>
#include <string.h>

#include <kernel/mem/paging.hpp>
#include <kernel/mem/slab.hpp>

using namespace kernel::mem::paging;

static struct zone_info {
    page* next;
    std::size_t count;
} zones[52];

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

// where order represents power of 2
page* _create_zone(pfn_t pfn, int order)
{
    page* zone = pfn_to_page(pfn);

    assert(zone->flags & PAGE_PRESENT);
    zone->flags |= PAGE_BUDDY;

    zone->next = zones[order].next;
    zones[order].next = zone;
    zones[order].count++;

    return zone;
}

void _split_zone(page* zone, int order, int target_order)
{
    while (order > target_order) {
        pfn_t pfn = page_to_pfn(zone);
        _create_zone(buddy(pfn, order - 1), order - 1);

        order--;
    }
}

page* _alloc_zone(int order)
{
    for (int i = order; i < 52; ++i) {
        if (zones[i].count == 0)
            continue;

        auto* zone = zones[i].next;
        zones[i].next = zone->next;
        zones[i].count--;

        // TODO: set free bitmap
        zone->refcount++;

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
    auto* zone = _alloc_zone(order);
    if (!zone) {
        // TODO: die
        return nullptr;
    }

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

pfn_t kernel::mem::paging::page_to_pfn(page* _page)
{
    return (pfn_t)(_page - PAGE_ARRAY) * 0x1000;
}

page* kernel::mem::paging::pfn_to_page(pfn_t pfn)
{
    return PAGE_ARRAY + pfn / 0x1000;
}
