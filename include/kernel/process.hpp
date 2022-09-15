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

class proclist;
class readyqueue;

inline process* volatile current_process;
inline thread* volatile current_thread;
inline proclist* procs;
inline readyqueue* readythds;

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

    inline thread(const thread& thd, process* new_parent)
        : thread { thd }
    {
        owner = new_parent;
    }

    constexpr ~thread()
    {
        if (kstack)
            free_n_raw_pages(to_page(kstack), 2);
    }
};

class thdlist {
public:
    using list_type = types::list<thread>;

private:
    list_type thds;

public:
    constexpr thdlist(const thdlist& obj) = delete;
    constexpr thdlist(thdlist&& obj) = delete;

    constexpr thdlist& operator=(const thdlist& obj) = delete;
    constexpr thdlist& operator=(thdlist&& obj) = delete;

    constexpr thdlist(thdlist&& obj, process* new_parent)
        : thds { types::move(obj.thds) }
    {
        for (auto& thd : thds)
            thd.owner = new_parent;
    }

    explicit constexpr thdlist(void)
    {
    }

    // implementation is below
    constexpr ~thdlist();

    template <typename... Args>
    constexpr thread& Emplace(Args&&... args)
    {
        return *thds.emplace_back(types::forward<Args>(args)...);
    }

    constexpr size_t size(void) const
    {
        return thds.size();
    }

    constexpr list_type& underlying_list(void)
    {
        return thds;
    }
};

class process {
public:
    mutable kernel::mm_list mms;
    thdlist thds;
    kernel::evtqueue wait_lst;
    process_attr attr;
    pid_t pid;
    pid_t ppid;

public:
    process(process&& val);
    process(const process&);

    explicit process(pid_t ppid, bool system = true);

    constexpr bool is_system(void) const
    {
        return attr.system;
    }
    constexpr bool is_zombie(void) const
    {
        return attr.zombie;
    }

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

extern "C" void NORETURN init_scheduler();
void schedule(void);

constexpr uint32_t push_stack(uint32_t** stack, uint32_t val)
{
    --*stack;
    **stack = val;
    return val;
}

// class thdlist
constexpr thdlist::~thdlist()
{
    for (auto iter = thds.begin(); iter != thds.end(); ++iter)
        readythds->remove_all(&iter);
}

void k_new_thread(void (*func)(void*), void* data);
