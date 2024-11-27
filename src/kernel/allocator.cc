#include <bit>
#include <cstddef>

#include <assert.h>
#include <stdint.h>

#include <types/allocator.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/slab.hpp>

constexpr uintptr_t KERNEL_HEAP_START = 0xffff'ff81'8000'0000;
constexpr uintptr_t KERNEL_HEAP_END = 0xffff'ffbf'ffff'ffff;
constexpr uintptr_t KERNEL_HEAP_SIZE = KERNEL_HEAP_END - KERNEL_HEAP_START;

namespace types::memory {

struct mem_blk_flags {
    unsigned long is_free : 8;
    unsigned long has_next : 8;
};

struct mem_blk {
    std::size_t size;
    mem_blk_flags flags;
    // the first byte of the memory space
    // the minimal allocated space is 8 bytes
    std::byte data[];
};

constexpr std::byte* aspbyte(void* pblk) {
    return std::bit_cast<std::byte*>(pblk);
}

constexpr mem_blk* aspblk(void* pbyte) {
    return std::bit_cast<mem_blk*>(pbyte);
}

constexpr mem_blk* next(mem_blk* blk, std::size_t blk_size) {
    auto* p = aspbyte(blk);
    p += sizeof(mem_blk);
    p += blk_size;
    return aspblk(p);
}

// blk MUST be free
constexpr void unite_afterwards(mem_blk* blk) {
    while (blk->flags.has_next) {
        auto* blk_next = next(blk, blk->size);
        if (!blk_next->flags.is_free)
            break;
        blk->size += sizeof(mem_blk) + blk_next->size;
        blk->flags.has_next = blk_next->flags.has_next;
    }
}

// @param start_pos position where to start finding
// @param size the size of the block we're looking for
// @return found block if suitable block exists, if not, the last block
constexpr mem_blk* find_blk(std::byte** p_start, std::size_t size) {
    mem_blk* start_pos = aspblk(*p_start);
    bool no_free_so_far = true;

    while (true) {
        if (start_pos->flags.is_free) {
            unite_afterwards(start_pos);

            no_free_so_far = false;

            if (start_pos->size >= size)
                break;
        }

        if (no_free_so_far)
            *p_start = aspbyte(start_pos);

        if (!start_pos->flags.has_next)
            break;
        start_pos = next(start_pos, start_pos->size);
    }
    return start_pos;
}

constexpr void split_block(mem_blk* blk, std::size_t this_size) {
    // block is too small to get split
    // that is, the block to be split should have enough room
    // for "this_size" bytes and also could contain a new block
    if (blk->size < this_size + sizeof(mem_blk) + 1024)
        return;

    mem_blk* blk_next = next(blk, this_size);

    blk_next->size = blk->size - this_size - sizeof(mem_blk);

    blk_next->flags.has_next = blk->flags.has_next;
    blk_next->flags.is_free = 1;

    blk->flags.has_next = 1;
    blk->size = this_size;
}

std::byte* brk_memory_allocator::brk(byte* addr) {
    if (addr >= p_limit)
        return nullptr;

    uintptr_t current_allocated = reinterpret_cast<uintptr_t>(p_allocated);
    uintptr_t new_brk = reinterpret_cast<uintptr_t>(addr);

    current_allocated &= ~(0x200000 - 1);
    new_brk &= ~(0x200000 - 1);

    using namespace kernel::mem::paging;
    while (current_allocated <= new_brk) {
        auto idx = idx_all(current_allocated);
        auto pdpt = KERNEL_PAGE_TABLE[std::get<1>(idx)].parse();

        auto pdpte = pdpt[std::get<2>(idx)];
        if (!pdpte.pfn())
            pdpte.set(PA_KERNEL_PAGE_TABLE, alloc_page_table());

        auto pde = pdpte.parse()[std::get<3>(idx)];
        assert(!(pde.attributes() & PA_P));
        pde.set(PA_KERNEL_DATA_HUGE, page_to_pfn(alloc_pages(9)));

        current_allocated += 0x200000;
    }
    p_allocated = (std::byte*)current_allocated;

    return p_break = addr;
}

std::byte* brk_memory_allocator::sbrk(size_type increment) {
    return brk(p_break + increment);
}

brk_memory_allocator::brk_memory_allocator(byte* start, size_type size)
    : p_start(start)
    , p_limit(start + size)
    , p_break(start)
    , p_allocated(start) {
    auto* p_blk = aspblk(brk(p_start));
    sbrk(sizeof(mem_blk) + 1024); // 1024 bytes (minimum size for a block)

    p_blk->size = 1024;
    p_blk->flags.has_next = 0;
    p_blk->flags.is_free = 1;
}

void* brk_memory_allocator::allocate(size_type size) {
    kernel::async::lock_guard_irq lck(mtx);
    // align to 1024 bytes boundary
    size = (size + 1024 - 1) & ~(1024 - 1);

    auto* block_allocated = find_blk(&p_start, size);
    if (!block_allocated->flags.has_next &&
        (!block_allocated->flags.is_free || block_allocated->size < size)) {
        // 'block_allocated' in the argument list is the pointer
        // pointing to the last block

        if (!sbrk(sizeof(mem_blk) + size))
            return nullptr;

        block_allocated->flags.has_next = 1;

        block_allocated = next(block_allocated, block_allocated->size);

        block_allocated->flags.has_next = 0;
        block_allocated->flags.is_free = 1;
        block_allocated->size = size;
    } else {
        split_block(block_allocated, size);
    }

    block_allocated->flags.is_free = 0;

    return block_allocated->data;
}

void brk_memory_allocator::deallocate(void* ptr) {
    kernel::async::lock_guard_irq lck(mtx);
    auto* blk = aspblk(aspbyte(ptr) - sizeof(mem_blk));

    blk->flags.is_free = 1;

    if (aspbyte(blk) < p_start)
        p_start = aspbyte(blk);

    // unite free blocks nearby
    unite_afterwards(blk);
}

bool brk_memory_allocator::allocated(void* ptr) const noexcept {
    return (void*)KERNEL_HEAP_START <= aspbyte(ptr) && aspbyte(ptr) < sbrk();
}

static brk_memory_allocator* k_alloc;

} // namespace types::memory

static kernel::mem::slab_cache caches[7];

static constexpr int __cache_index(std::size_t size) {
    if (size <= 32)
        return 0;
    if (size <= 64)
        return 1;
    if (size <= 96)
        return 2;
    if (size <= 128)
        return 3;
    if (size <= 192)
        return 4;
    if (size <= 256)
        return 5;
    if (size <= 512)
        return 6;
    return -1;
}

void kernel::kinit::init_allocator() {
    mem::init_slab_cache(caches + 0, 32);
    mem::init_slab_cache(caches + 1, 64);
    mem::init_slab_cache(caches + 2, 96);
    mem::init_slab_cache(caches + 3, 128);
    mem::init_slab_cache(caches + 4, 192);
    mem::init_slab_cache(caches + 5, 256);
    mem::init_slab_cache(caches + 6, 512);

    types::memory::k_alloc = new types::memory::brk_memory_allocator(
        (std::byte*)KERNEL_HEAP_START, KERNEL_HEAP_SIZE);
}

extern "C" void* _do_allocate(uintptr_t size) {
    int idx = __cache_index(size);
    void* ptr = nullptr;
    if (idx < 0)
        ptr = types::memory::k_alloc->allocate(size);
    else
        ptr = kernel::mem::slab_alloc(&caches[idx]);

    return ptr;
}

// return 0 if deallocate successfully
// return -1 if ptr is nullptr
// return -2 if size is not correct for slab allocated memory
extern "C" int32_t _do_deallocate(void* ptr, uintptr_t size) {
    if (!ptr)
        return -1;

    if (types::memory::k_alloc->allocated(ptr)) {
        types::memory::k_alloc->deallocate(ptr);
        return 0;
    }

    int idx = __cache_index(size);
    if (idx < 0)
        return -2;

    kernel::mem::slab_free(ptr);

    return 0;
}

void* operator new(size_t size) {
    auto* ret = _do_allocate(size);
    assert(ret);

    return ret;
}

void operator delete(void* ptr) {
    if (!ptr)
        return;

    if (types::memory::k_alloc->allocated(ptr))
        types::memory::k_alloc->deallocate(ptr);
    else
        kernel::mem::slab_free(ptr);
}

void operator delete(void* ptr, std::size_t size) {
    if (!ptr)
        return;

    int ret = _do_deallocate(ptr, size);

    assert(ret == 0);
}

void* operator new[](size_t sz) {
    return ::operator new(sz);
}

void operator delete[](void* ptr) {
    ::operator delete(ptr);
}

void operator delete[](void* ptr, std::size_t size) {
    ::operator delete(ptr, size);
}
