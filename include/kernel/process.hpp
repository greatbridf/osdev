#pragma once

#include <kernel/event/evtqueue.hpp>
#include <kernel/interrupt.h>
#include <kernel/mm.hpp>
#include <kernel/task.h>
#include <types/cplusplus.hpp>
#include <types/hash_map.hpp>
#include <types/list.hpp>
#include <types/stdint.h>
#include <types/types.h>

typedef size_t pid_t;

class process;
struct thread;

struct process_attr {
    uint16_t system : 1;
    uint16_t zombie : 1 = 0;
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
    }
};

class process {
public:
    mutable kernel::mm_list mms;
    types::list<thread> thds;
    kernel::evtqueue wait_lst;
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

    ~process();

private:
    static inline pid_t max_pid;

    static inline pid_t alloc_pid(void)
    {
        return ++max_pid;
    }
};

class proclist {
public:
    using list_type = types::list<process>;
    using index_type = types::hash_map<pid_t, types::list<process>::iterator_type, types::linux_hasher<pid_t>>;
    using child_index_type = types::hash_map<pid_t, types::list<pid_t>, types::linux_hasher<pid_t>>;
    using iterator_type = list_type::iterator_type;
    using const_iterator_type = list_type::const_iterator_type;

private:
    list_type m_procs;
    index_type m_idx;
    child_index_type m_child_idx;

public:
    template <typename... Args>
    constexpr iterator_type emplace(Args&&... args)
    {
        auto iter = m_procs.emplace_back(types::forward<Args>(args)...);
        m_idx.insert(iter->pid, iter);

        auto children = m_child_idx.find(iter->ppid);
        if (!children) {
            m_child_idx.insert(iter->ppid, {});
            children = m_child_idx.find(iter->ppid);
        }

        children->value.push_back(iter->pid);

        return iter;
    }

    constexpr void remove(pid_t pid)
    {
        make_children_orphans(pid);

        auto proc_iter = m_idx.find(pid);
        auto ppid = proc_iter->value->ppid;

        auto& parent_children = m_child_idx.find(ppid)->value;

        auto i = parent_children.find(pid);
        parent_children.erase(i);

        m_procs.erase(proc_iter->value);
        m_idx.remove(proc_iter);
    }

    constexpr process* find(pid_t pid)
    {
        return m_idx.find(pid)->value.ptr();
    }

    constexpr bool has_child(pid_t pid)
    {
        auto children = m_child_idx.find(pid);
        return children && !children->value.empty();
    }

    constexpr void make_children_orphans(pid_t pid)
    {
        auto children = m_child_idx.find(pid);
        if (children) {
            auto init_children = m_child_idx.find(1);
            for (auto iter = children->value.begin(); iter != children->value.end(); ++iter) {
                init_children->value.push_back(*iter);
                this->find(*iter)->ppid = 1;
            }
            m_child_idx.remove(children);
        }
    }
};

inline process* volatile current_process;
inline thread* volatile current_thread;
inline proclist* procs;

extern "C" void NORETURN init_scheduler();
void schedule(void);

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

constexpr uint32_t push_stack(uint32_t** stack, uint32_t val)
{
    --*stack;
    **stack = val;
    return val;
}

void k_new_thread(void (*func)(void*), void* data);
