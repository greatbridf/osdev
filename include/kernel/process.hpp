#pragma once

#include <kernel/interrupt.h>
#include <kernel/mm.hpp>
#include <kernel/task.h>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/stdint.h>
#include <types/types.h>

typedef size_t pid_t;

class process;
struct thread;

struct process_attr {
    uint16_t system : 1;
};

struct thread_attr {
    uint32_t system : 1;
    uint32_t ready : 1;
    uint32_t wait : 1;
};

struct thread {
private:
    inline void alloc_kstack(void)
    {
        // TODO: alloc low mem
        kstack = to_pp(alloc_n_raw_pages(2));
        kstack += THREAD_KERNEL_STACK_SIZE;
        esp = reinterpret_cast<uint32_t*>(kstack);
    }

public:
    uint32_t* esp;
    pptr_t kstack;
    process* owner;
    thread_attr attr;

    explicit inline thread(process* _owner, bool system)
        : owner { _owner }
        , attr {
            .system = system,
            .ready = 1,
            .wait = 0,
        }
    {
        alloc_kstack();
    }

    constexpr thread(thread&& val)
        : esp { val.esp }
        , kstack { val.kstack }
        , owner { val.owner }
        , attr { val.attr }
    {
        val.attr = {};
        val.esp = 0;
        val.kstack = 0;
        val.owner = nullptr;
    }

    inline thread(const thread& val)
        : owner { val.owner }
        , attr { val.attr }
    {
        alloc_kstack();
    }

    constexpr ~thread()
    {
        if (kstack)
            free_n_raw_pages(to_page(kstack), 2);
        memset(this, 0x00, sizeof(thread));
    }
};

class process {
public:
    mutable kernel::mm_list mms;
    types::list<thread> thds;
    process_attr attr;
    pid_t pid;
    pid_t ppid;

public:
    process(process&& val);
    process(const process&) = delete;
    process(const process& proc, const thread& main_thread);

    // only used for system initialization
    explicit process(void);
    explicit process(void (*func_in_kernel_space)(void), pid_t ppid);

private:
    static inline pid_t max_pid;

    static inline pid_t alloc_pid(void)
    {
        return ++max_pid;
    }
};

constexpr uint32_t push_stack(uint32_t** stack, uint32_t val)
{
    --*stack;
    **stack = val;
    return val;
}

inline process* volatile current_process;
inline thread* volatile current_thread;
inline typename types::hash_map<pid_t, types::list<pid_t>, types::linux_hasher<pid_t>>* idx_child_processes;

extern "C" void NORETURN init_scheduler();
void schedule(void);

pid_t add_to_process_list(process&& proc);

void add_to_ready_list(thread* thd);
void remove_from_ready_list(thread* thd);
types::list<thread*>::iterator_type query_next_thread(void);

// the function call INVALIDATES iterator
inline void next_task(types::list<thread*>::iterator_type target)
{
    auto* ptr = *target;
    remove_from_ready_list(ptr);
    if (ptr->attr.ready)
        add_to_ready_list(ptr);
}

process* findproc(pid_t pid);

void k_new_thread(void (*func)(void*), void* data);
