#pragma once

#include <cstddef>
#include <type_traits>

#include <stdint.h>

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

void init_slab_cache(slab_cache* cache, std::size_t obj_size);

void* slab_alloc(slab_cache* cache);
void slab_free(void* ptr);

} // namespace kernel::mem
