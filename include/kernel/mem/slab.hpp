#pragma once

#include <cstddef>
#include <type_traits>

#include <stdint.h>

#include "paging.hpp"
#include "phys.hpp"

namespace kernel::mem {

struct slab_cache;

struct slab_head {
    slab_cache* cache;

    slab_head* next;
    slab_head* prev;

    void* free;

    unsigned int free_count;
    unsigned int obj_size;
};

struct slab_cache {
    slab_head* slabs_empty;
    slab_head* slabs_partial;
    slab_head* slabs_full;

    std::size_t obj_size;
};

template <typename T>
class slab_allocator {
    using value_type = T;
    using propagate_on_container_move_assignment = std::true_type;

    // throws std::bad_alloc
    [[nodiscard]] constexpr T* allocate(std::size_t n)
    { return static_cast<T*>(::operator new(n * sizeof(T))); }

    // TODO: check allocated size
    constexpr void deallocate(T* ptr, std::size_t)
    { ::operator delete(ptr); }
};

void init_slab_cache(slab_cache* cache, std::size_t obj_size);
void slab_add_page(slab_cache* cache, paging::pfn_t pfn);

void* slab_alloc(slab_cache* cache);
void slab_free(void* ptr);

} // namespace kernel::mem
