#include <cstddef>

#include <assert.h>

#include <kernel/mem/paging.hpp>
#include <kernel/mem/slab.hpp>

using namespace kernel::mem;

constexpr std::size_t SLAB_PAGE_SIZE = 0x1000; // 4K

template <typename ListNode>
void list_insert(ListNode** head, ListNode* node)
{
    node->next = *head;
    if (*head)
        (*head)->prev = node;
    *head = node;
}

template <typename ListNode>
ListNode* list_get(ListNode** head)
{
    ListNode* node = *head;
    if (node) {
        *head = node->next;

        node->next = nullptr;
        node->prev = nullptr;
    }
    return node;
}

template <typename ListNode>
void list_remove(ListNode** head, ListNode* node)
{
    if (node->prev)
        node->prev->next = node->next;
    else
        *head = node->next;

    if (node->next)
        node->next->prev = node->prev;

    node->next = nullptr;
    node->prev = nullptr;
}

std::ptrdiff_t _slab_data_start_offset(std::size_t size)
{
    return (sizeof(slab_head) + size - 1) & ~(size - 1);
}

std::size_t _slab_max_count(std::size_t size)
{
    return (SLAB_PAGE_SIZE - _slab_data_start_offset(size)) / size;
}

void* _slab_head_alloc(slab_head* slab)
{
    if (slab->free_count == 0)
        return nullptr;

    void* ptr = slab->free;
    slab->free = *(void**)ptr;
    slab->free_count--;

    return ptr;
}

slab_head* _make_slab(uintptr_t start, std::size_t size)
{
    slab_head* slab = physaddr<slab_head>{start};

    slab->obj_size = size;
    slab->free_count = _slab_max_count(size);
    slab->next = nullptr;
    slab->prev = nullptr;

    slab->free = physaddr<void>{start + _slab_data_start_offset(size)};

    std::byte* ptr = (std::byte*)slab->free;
    for (unsigned i = 0; i < slab->free_count; ++i) {
        if (i == slab->free_count-1)
            *(void**)ptr = nullptr;
        else
            *(void**)ptr = ptr + size;
        ++ptr;
    }

    return slab;
}

void* kernel::mem::slab_alloc(slab_cache* cache) {
    slab_head* slab = cache->slabs_partial;
    if (!slab) { // no partial slabs, try to get an empty slab
        if (!cache->slabs_empty) // no empty slabs, create a new one
            slab_add_page(cache, paging::page_to_pfn(paging::alloc_page()));

        slab = list_get(&cache->slabs_empty);

        list_insert(&cache->slabs_partial, slab);
    }

    void* ptr = _slab_head_alloc(slab);

    if (slab->free_count == 0) { // slab is full
        list_remove(&cache->slabs_partial, slab);
        list_insert(&cache->slabs_full, slab);
    }

    return ptr;
}

void kernel::mem::slab_free(void* ptr) {
    slab_head* slab = (slab_head*)((uintptr_t)ptr & ~(SLAB_PAGE_SIZE-1));

    *(void**)ptr = slab->free;
    slab->free = ptr;
    slab->free_count++;

    if (slab->free_count == _slab_max_count(slab->obj_size)) {
        auto* cache = slab->cache;
        slab_head** head = nullptr;

        if (cache->slabs_full == slab) {
            head = &cache->slabs_full;
        } else {
            assert(cache->slabs_partial == slab);
            head = &cache->slabs_partial;
        }

        list_remove(head, slab);
        list_insert(&cache->slabs_empty, slab);
    }
}

void kernel::mem::slab_add_page(slab_cache* cache, paging::pfn_t pfn) {
    auto slab = _make_slab(pfn, cache->obj_size);
    slab->cache = cache;

    list_insert(&cache->slabs_empty, slab);
}

void kernel::mem::init_slab_cache(slab_cache* cache, std::size_t obj_size)
{
    cache->obj_size = obj_size;
    cache->slabs_empty = nullptr;
    cache->slabs_partial = nullptr;
    cache->slabs_full = nullptr;
}
