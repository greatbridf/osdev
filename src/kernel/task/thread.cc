#include <kernel/task/thread.hpp>

#include <queue>

#include <types/lock.hpp>

#include <kernel/log.hpp>
#include <kernel/mm.hpp>
#include <kernel/signal.hpp>
#include <kernel/task/readyqueue.hpp>

using namespace kernel::task;

thread::thread(types::string<> name, pid_t owner)
    : owner { owner }, attr { READY | SYSTEM }, name { name }
{
}

thread::thread(const thread& val, pid_t owner)
    : owner { owner }, attr { val.attr }, name { val.name }
{
}

tid_t thread::tid() const
{
    return (tid_t)kstack.stack_base;
}

bool thread::operator<(const thread& rhs) const
{
    return tid() < rhs.tid();
}

bool thread::operator==(const thread& rhs) const
{
    return tid() == rhs.tid();
}

static std::priority_queue<std::byte*> s_kstacks;

thread::kernel_stack::kernel_stack()
{
    static int allocated;
    static types::mutex mtx;
    types::lock_guard lck(mtx);

    if (!s_kstacks.empty()) {
        stack_base = s_kstacks.top();
        esp = (uint32_t*)stack_base;
        s_kstacks.pop();
        return;
    }

    // kernel stack pt is at page#0x00005
    kernel::paccess pa(0x00005);
    auto pt = (pt_t)pa.ptr();
    assert(pt);

    int cnt = THREAD_KERNEL_STACK_SIZE / PAGE_SIZE;
    pte_t* pte = *pt + allocated * cnt;

    for (int i = 0; i < cnt; ++i) {
        pte[i].v = 0x3;
        pte[i].in.page = __alloc_raw_page();
    }

    stack_base = (std::byte*)(0xffc00000 + THREAD_KERNEL_STACK_SIZE * (allocated + 1));
    esp = (uint32_t*)stack_base;

    ++allocated;
}

thread::kernel_stack::kernel_stack(const kernel_stack& other)
    : kernel_stack()
{
    auto offset = vptrdiff(other.stack_base, other.esp);
    esp = (uint32_t*)(stack_base - offset);
    memcpy(esp, other.esp, offset);
}

thread::kernel_stack::kernel_stack(kernel_stack&& other)
    : stack_base(std::exchange(other.stack_base, nullptr))
    , esp(std::exchange(other.esp, nullptr)) { }

thread::kernel_stack::~kernel_stack()
{
    s_kstacks.push(stack_base);
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

        if (attr & READY) {
            kmsgf("[kernel:warn] pid%d tries to wake up from ready state", owner);
            break;
        }

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
        void* dst = (void*)ptr->base_addr;
        std::size_t len = ptr->limit;
        if (len > 0 && dst)
            memset(dst, 0x00, len);
        return 0;
    }

    if (ptr->entry_number == -1U)
        ptr->entry_number = 6;
    else
        return -1;

    tls_desc.limit_low = ptr->limit & 0xFFFF;
    tls_desc.base_low = ptr->base_addr & 0xFFFF;
    tls_desc.base_mid = (ptr->base_addr >> 16) & 0xFF;
    tls_desc.access = SD_TYPE_DATA_USER;
    tls_desc.limit_high = (ptr->limit >> 16) & 0xF;
    tls_desc.flags = (ptr->limit_in_pages << 3) | (ptr->seg_32bit << 2);
    tls_desc.base_high = (ptr->base_addr >> 24) & 0xFF;

    return 0;
}

int thread::load_thread_area() const
{
    if (tls_desc.flags == 0)
        return -1;
    kernel::user::load_thread_area(tls_desc);
    return 0;
}
