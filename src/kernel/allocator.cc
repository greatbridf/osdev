#include <types/allocator.hpp>

#include <bit>
#include <cstddef>

#include <assert.h>
#include <stdint.h>

#include <kernel/async/lock.hpp>

namespace types::memory {

struct mem_blk_flags {
    uint8_t is_free;
    uint8_t has_next;
    uint8_t : 8; // unused1
    uint8_t : 8; // unused2
};

struct mem_blk {
    std::size_t size;
    mem_blk_flags flags;
    // the first byte of the memory space
    // the minimal allocated space is 8 bytes
    std::byte data[];
};

constexpr std::byte* aspbyte(void* pblk)
{ return std::bit_cast<std::byte*>(pblk);}

constexpr mem_blk* aspblk(void* pbyte)
{ return std::bit_cast<mem_blk*>(pbyte);}

constexpr mem_blk* next(mem_blk* blk, std::size_t blk_size)
{
    auto* p = aspbyte(blk);
    p += sizeof(mem_blk);
    p += blk_size;
    return aspblk(p);
}

// blk MUST be free
constexpr void unite_afterwards(mem_blk* blk)
{
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
constexpr mem_blk* find_blk(std::byte** p_start, std::size_t size)
{
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

constexpr void split_block(mem_blk* blk, std::size_t this_size)
{
    // block is too small to get split
    // that is, the block to be split should have enough room
    // for "this_size" bytes and also could contain a new block
    if (blk->size < this_size + sizeof(mem_blk) + 8)
        return;

    mem_blk* blk_next = next(blk, this_size);

    blk_next->size = blk->size
        - this_size
        - sizeof(mem_blk);

    blk_next->flags.has_next = blk->flags.has_next;
    blk_next->flags.is_free = 1;

    blk->flags.has_next = 1;
    blk->size = this_size;
}

brk_memory_allocator::brk_memory_allocator(byte* start, size_type size)
    : p_start(start)
    , p_limit(start + size)
{
    brk(p_start);
    auto* p_blk = aspblk(sbrk(0));
    p_blk->size = 8;
    p_blk->flags.has_next = 0;
    p_blk->flags.is_free = 1;
}

void* brk_memory_allocator::allocate(size_type size)
{
    kernel::async::lock_guard_irq lck(mtx);
    // align to 8 bytes boundary
    size = (size + 7) & ~7;

    auto* block_allocated = find_blk(&p_start, size);
    if (!block_allocated->flags.has_next
        && (!block_allocated->flags.is_free || block_allocated->size < size)) {
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

void brk_memory_allocator::deallocate(void* ptr)
{
    kernel::async::lock_guard_irq lck(mtx);
    auto* blk = aspblk(aspbyte(ptr) - sizeof(mem_blk));

    blk->flags.is_free = 1;

    if (aspbyte(blk) < p_start)
        p_start = aspbyte(blk);

    // unite free blocks nearby
    unite_afterwards(blk);
}

static std::byte ki_heap[0x100000];
static brk_memory_allocator ki_alloc(ki_heap, sizeof(ki_heap));
static brk_memory_allocator* k_alloc;

void* kimalloc(std::size_t size)
{
    return ki_alloc.allocate(size);
}

void kifree(void* ptr)
{
    ki_alloc.deallocate(ptr);
}

} // namespace types::memory

SECTION(".text.kinit")
void kernel::kinit::init_kernel_heap(void *start, std::size_t size)
{
    using namespace types::memory;
    k_alloc = kinew<brk_memory_allocator>((std::byte*)start, size);
}

void* operator new(size_t sz)
{
    void* ptr = types::memory::k_alloc->allocate(sz);
    assert(ptr);
    return ptr;
}

void* operator new[](size_t sz)
{
    void* ptr = types::memory::k_alloc->allocate(sz);
    assert(ptr);
    return ptr;
}

void operator delete(void* ptr)
{
    types::memory::k_alloc->deallocate(ptr);
}

void operator delete(void* ptr, size_t)
{
    types::memory::k_alloc->deallocate(ptr);
}

void operator delete[](void* ptr)
{
    types::memory::k_alloc->deallocate(ptr);
}

void operator delete[](void* ptr, size_t)
{
    types::memory::k_alloc->deallocate(ptr);
}
