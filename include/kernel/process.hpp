#pragma once

#include "types/map.hpp"
#include "types/pair.hpp"
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
    explicit process(pid_t ppid);
    explicit process(void (*func_in_kernel_space)(void), pid_t ppid);

    ~process();

private:
    static inline pid_t max_pid;

    static inline pid_t alloc_pid(void)
    {
        return ++max_pid;
    }
};

class proclist final {
public:
    using list_type = types::map<pid_t, process>;
    using child_index_type = types::hash_map<pid_t, types::list<pid_t>, types::linux_hasher<pid_t>>;
    using iterator_type = list_type::iterator_type;
    using const_iterator_type = list_type::const_iterator_type;

private:
    list_type m_procs;
    child_index_type m_child_idx;

public:
    template <typename... Args>
    iterator_type emplace(Args&&... args)
    {
        process _proc(types::forward<Args>(args)...);
        auto pid = _proc.pid;
        auto ppid = _proc.ppid;
        auto iter = m_procs.insert(types::make_pair(pid, types::move(_proc)));

        auto children = m_child_idx.find(ppid);
        if (!children) {
            m_child_idx.emplace(ppid, types::list<pid_t> {});
            children = m_child_idx.find(ppid);
        }

        children->value.push_back(pid);

        return iter;
    }

    constexpr void remove(pid_t pid)
    {
        make_children_orphans(pid);

        auto proc_iter = m_procs.find(pid);
        auto ppid = proc_iter->value.ppid;

        auto& parent_children = m_child_idx.find(ppid)->value;

        auto i = parent_children.find(pid);
        parent_children.erase(i);

        m_procs.erase(proc_iter);
    }

    constexpr process* find(pid_t pid)
    {
        return &m_procs.find(pid)->value;
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

class readyqueue final {
public:
    using list_type = types::list<thread*>;
    using iterator_type = list_type::iterator_type;
    using const_iterator_type = list_type::const_iterator_type;

private:
    list_type m_thds;

private:
    readyqueue(const readyqueue&) = delete;
    readyqueue(readyqueue&&) = delete;
    readyqueue& operator=(const readyqueue&) = delete;
    readyqueue& operator=(readyqueue&&) = delete;

    ~readyqueue() = delete;

public:
    constexpr explicit readyqueue(void) = default;

    constexpr void push(thread* thd)
    {
        m_thds.push_back(thd);
    }

    constexpr thread* pop(void)
    {
        auto iter = m_thds.begin();
        while (!((*iter)->attr.ready))
            iter = m_thds.erase(iter);
        auto* ptr = *iter;
        m_thds.erase(iter);
        return ptr;
    }

    constexpr thread* query(void)
    {
        auto* thd = this->pop();
        this->push(thd);
        return thd;
    }

    constexpr void remove_all(thread* thd)
    {
        auto iter = m_thds.find(thd);
        while (iter != m_thds.end()) {
            m_thds.erase(iter);
            iter = m_thds.find(thd);
        }
    }
};

inline process* volatile current_process;
inline thread* volatile current_thread;
inline proclist* procs;
inline readyqueue* readythds;

extern "C" void NORETURN init_scheduler();
void schedule(void);

constexpr uint32_t push_stack(uint32_t** stack, uint32_t val)
{
    --*stack;
    **stack = val;
    return val;
}

void k_new_thread(void (*func)(void*), void* data);
