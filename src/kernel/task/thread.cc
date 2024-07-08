#include <queue>

#include <stdint.h>

#include <types/types.h>

#include <kernel/async/lock.hpp>
#include <kernel/log.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/signal.hpp>
#include <kernel/task/readyqueue.hpp>
#include <kernel/task/thread.hpp>

constexpr std::size_t KERNEL_STACK_ORDER = 3; // 2^3 * 4096 = 32KB

using namespace kernel::task;
using namespace kernel::mem;
using namespace kernel::mem::paging;

struct PACKED tss64_t {
    uint32_t _reserved1;
    uint64_t rsp[3];
    uint64_t _reserved2;
    uint64_t ist[7];
    uint64_t _reserved3;
    uint32_t _reserved4;
};
constexpr physaddr<tss64_t> tss{0x00000070};

thread::thread(std::string name, pid_t owner)
    : owner { owner }, attr { READY | SYSTEM }, name { name } { }

thread::thread(const thread& val, pid_t owner)
    : owner { owner }, attr { val.attr }, name { val.name }, tls_desc32{val.tls_desc32} { }

tid_t thread::tid() const
{
    return (tid_t)kstack.pfn;
}

bool thread::operator<(const thread& rhs) const
{
    return tid() < rhs.tid();
}

bool thread::operator==(const thread& rhs) const
{
    return tid() == rhs.tid();
}

static inline uintptr_t __stack_bottom(pfn_t pfn)
{
    return (uintptr_t)(void*)
        kernel::mem::physaddr<void>{pfn + (1 << KERNEL_STACK_ORDER) * 0x1000};
}

thread::kernel_stack::kernel_stack()
{
    pfn = page_to_pfn(alloc_pages(KERNEL_STACK_ORDER));
    sp = __stack_bottom(pfn);
}

thread::kernel_stack::kernel_stack(const kernel_stack& other)
    : kernel_stack()
{
    auto offset = __stack_bottom(other.pfn) - other.sp;

    sp -= offset;
    memcpy((void*)sp, (void*)other.sp, offset);
}

thread::kernel_stack::kernel_stack(kernel_stack&& other)
    : pfn(std::exchange(other.pfn, 0))
    , sp(std::exchange(other.sp, 0)) { }

thread::kernel_stack::~kernel_stack()
{
    if (!pfn)
        return;
    free_pages(pfn, KERNEL_STACK_ORDER);
}

uint64_t thread::kernel_stack::pushq(uint64_t val)
{
    sp -= 8;
    *(uint64_t*)sp = val;
    return val;
}

uint32_t thread::kernel_stack::pushl(uint32_t val)
{
    sp -= 4;
    *(uint32_t*)sp = val;
    return val;
}

void thread::kernel_stack::load_interrupt_stack() const
{
    tss->rsp[0] = sp;
}

void thread::set_attr(thd_attr_t new_attr)
{
    switch (new_attr) {
    case SYSTEM:
        attr |= SYSTEM;
        break;
    case READY:
        if (attr & ZOMBIE) {
            kmsgf("[kernel:warn] zombie process pid%d tries to wake up", owner);
            break;
        }

        if (attr & READY)
            break;

        attr &= SYSTEM;
        attr |= READY;

        dispatcher::enqueue(this);
        break;
    case ISLEEP:
        attr &= SYSTEM;
        attr |= ISLEEP;

        dispatcher::dequeue(this);
        break;
    case STOPPED:
        attr &= SYSTEM;
        attr |= STOPPED;

        dispatcher::dequeue(this);
        break;
    case ZOMBIE:
        attr &= SYSTEM;
        attr |= ZOMBIE;

        dispatcher::dequeue(this);
        break;
    default:
        kmsgf("[kernel:warn] unknown thread attribute: %x", new_attr);
        break;
    }
}

void thread::send_signal(signal_list::signo_type signal)
{
    if (signals.raise(signal))
        this->set_attr(READY);
}

int thread::set_thread_area(kernel::user::user_desc* ptr)
{
    if (ptr->read_exec_only && ptr->seg_not_present) {
        // TODO: use copy_to_user
        auto* dst = (void*)(uintptr_t)ptr->base_addr;
        std::size_t len = ptr->limit;
        if (len > 0 && dst)
            memset(dst, 0x00, len);
        return 0;
    }

    if (ptr->entry_number == -1U)
        ptr->entry_number = 7;
    else
        return -1;

    if (!ptr->seg_32bit)
        return -1;

    if ((ptr->limit & 0xffff) != 0xffff) {
        asm volatile("nop": : : "memory");
    }

    tls_desc32  = ptr->limit & 0x0'ffff;
    tls_desc32 |= (ptr->base_addr & 0x00'ffffffULL) << 16;
    tls_desc32 |= 0x4'0'f2'000000'0000;
    tls_desc32 |= (ptr->limit & 0xf'0000ULL) << (48-16);
    tls_desc32 |= ((ptr->limit_in_pages + 0ULL) << 55);
    tls_desc32 |= (ptr->base_addr & 0xff'000000ULL) << (56-24);

    return 0;
}

int thread::load_thread_area32() const
{
    kernel::user::load_thread_area32(tls_desc32);
    return 0;
}
