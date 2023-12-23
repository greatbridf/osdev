#pragma once

#include <map>
#include <list>
#include <memory>
#include <queue>
#include <set>
#include <tuple>
#include <utility>

#include <errno.h>
#include <fcntl.h>
#include <kernel/event/evtqueue.hpp>
#include <kernel/interrupt.h>
#include <kernel/mm.hpp>
#include <kernel/signal.hpp>
#include <kernel/task.h>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <sys/types.h>
#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/hash_map.hpp>
#include <types/path.hpp>
#include <types/status.h>
#include <types/string.hpp>
#include <types/types.h>

class process;

namespace kernel::tasks {

struct thread;

} // namespace kernel::tasks

class proclist;
class readyqueue;

inline process* volatile current_process;
inline kernel::tasks::thread* volatile current_thread;
inline proclist* procs;
inline readyqueue* readythds;

inline tss32_t tss;

struct process_attr {
    uint16_t system : 1;
    uint16_t zombie : 1 = 0;
};

struct thread_attr {
    uint32_t system : 1;
    uint32_t ready : 1;
};

namespace kernel::tasks {

using tid_t = uint32_t;

struct thread {
private:
    void alloc_kstack(void);
    void free_kstack(uint32_t p);

public:
    uint32_t* esp;
    uint32_t pkstack;
    pid_t owner;
    thread_attr attr;
    signal_list signals;

    int* __user set_child_tid {};
    int* __user clear_child_tid {};

    types::string<> name {};

    explicit inline thread(types::string<> name, pid_t owner)
        : owner { owner }
        , attr { .system = 1, .ready = 1, }
        , name { name }
    {
        alloc_kstack();
    }

    inline thread(const thread& val, pid_t owner)
        : owner { owner }, attr { val.attr }, name { val.name }
    {
        alloc_kstack();
    }

    constexpr void sleep()
    { attr.ready = 0; }
    void wakeup();
    constexpr bool is_ready() const
    { return attr.ready; }

    void send_signal(kernel::signal_list::signo_type signal);

    constexpr thread(thread&& val) = default;
    inline ~thread() { free_kstack(pkstack); }

    constexpr tid_t tid() const { return pkstack; }

    constexpr bool operator==(const thread& rhs) const
    { return pkstack == rhs.pkstack; }
    constexpr bool operator<(const thread& rhs) const
    { return pkstack < rhs.pkstack; }
};

}

class filearr {
private:
    // TODO: change this
    struct fditem {
        int flags;
        std::shared_ptr<fs::file> file;
    };

    std::map<int, fditem> arr;
    int min_avail { };

private:
    int allocate_fd(int from);
    void release_fd(int fd);
    inline int next_fd() { return allocate_fd(min_avail); }

public:
    constexpr filearr() = default;
    constexpr filearr(const filearr& val) = default;
    constexpr filearr(filearr&& val) = default;

    constexpr filearr& operator=(const filearr&) = delete;
    constexpr filearr& operator=(filearr&&) = delete;

    // TODO: the third parameter should be int flags
    //       determining whether the fd should be closed
    //       after exec() (FD_CLOEXEC)
    int dup2(int old_fd, int new_fd);
    int dup(int old_fd);

    int dupfd(int fd, int minfd, int flags);

    int set_flags(int fd, int flags);
    int clear_flags(int fd, int flags);

    constexpr fs::file* operator[](int i) const
    {
        auto iter = arr.find(i);
        if (!iter)
            return nullptr;
        return iter->second.file.get();
    }

    int pipe(int pipefd[2])
    {
        std::shared_ptr<fs::pipe> ppipe { new fs::pipe };

        bool inserted = false;
        int fd = next_fd();
        std::tie(std::ignore, inserted) = arr.emplace(fd, fditem {
            0, std::shared_ptr<fs::file> {
                new fs::fifo_file(nullptr, {
                    .read = 1,
                    .write = 0,
                }, ppipe),
        } } );
        assert(inserted);

        // TODO: use copy_to_user()
        pipefd[0] = fd;

        fd = next_fd();
        std::tie(std::ignore, inserted) = arr.emplace(fd, fditem {
            0, std::shared_ptr<fs::file> {
                new fs::fifo_file(nullptr, {
                    .read = 0,
                    .write = 1,
                }, ppipe),
        } } );
        assert(inserted);

        // TODO: use copy_to_user()
        pipefd[1] = fd;

        return 0;
    }

    int open(const process& current, const types::path& filepath, int flags, mode_t mode);

    constexpr void close(int fd)
    {
        auto iter = arr.find(fd);
        if (!iter)
            return;

        release_fd(fd);
        arr.erase(iter);
    }

    constexpr void onexec()
    {
        for (auto iter = arr.begin(); iter != arr.end(); ) {
            if (!(iter->second.flags & O_CLOEXEC)) {
                ++iter;
                continue;
            }
            release_fd(iter->first);
            iter = arr.erase(iter);
        }
    }

    constexpr void close_all(void)
    {
        for (const auto& item : arr)
            release_fd(item.first);
        arr.clear();
    }

    constexpr ~filearr()
    {
        close_all();
    }
};

class process {
public:
    struct wait_obj {
        pid_t pid;
        int code;
    };

public:
    kernel::memory::mm_list mms {};
    std::set<kernel::tasks::thread> thds;
    kernel::cond_var cv_wait;
    std::list<wait_obj> waitlist;
    process_attr attr {};
    filearr files;
    types::path pwd;
    mode_t umask { 0022 };

    pid_t pid {};
    pid_t ppid {};
    pid_t pgid {};
    pid_t sid {};

    tty* control_tty {};
    fs::vfs::dentry* root { fs::fs_root };
    std::set<pid_t> children;

public:
    process(const process&) = delete;
    explicit process(const process& parent, pid_t pid);

    // this function is used for system initialization
    // DO NOT use this after the system is on
    explicit process(pid_t pid, pid_t ppid);

    constexpr bool is_system(void) const
    { return attr.system; }
    constexpr bool is_zombie(void) const
    { return attr.zombie; }

    void send_signal(kernel::signal_list::signo_type signal);
};

class proclist final {
public:
    using list_type = std::map<pid_t, process>;
    using iterator = list_type::iterator;
    using const_iterator = list_type::const_iterator;

private:
    list_type m_procs;
    pid_t m_nextpid = 1;

    constexpr pid_t next_pid() { return m_nextpid++; }

public:
    process& emplace(pid_t ppid)
    {
        pid_t pid = next_pid();
        auto [ iter, inserted ] = m_procs.try_emplace(pid, pid, ppid);
        assert(inserted);

        if (try_find(ppid)) {
            bool success = false;
            std::tie(std::ignore, success) =
                find(ppid).children.insert(pid);
            assert(success);
        }

        return iter->second;
    }

    process& copy_from(process& proc)
    {
        pid_t pid = next_pid();
        auto [ iter, inserted ] = m_procs.try_emplace(pid, proc, pid);
        assert(inserted);

        proc.children.insert(pid);
        return iter->second;
    }

    constexpr void remove(pid_t pid)
    {
        make_children_orphans(pid);

        auto proc_iter = m_procs.find(pid);

        auto ppid = proc_iter->second.ppid;
        find(ppid).children.erase(pid);

        m_procs.erase(proc_iter);
    }

    constexpr bool try_find(pid_t pid) const
    { return m_procs.find(pid); }

    // if process doesn't exist, the behavior is undefined
    constexpr process& find(pid_t pid)
    {
        auto iter = m_procs.find(pid);
        assert(iter);
        return iter->second;
    }

    constexpr void make_children_orphans(pid_t pid)
    {
        auto& children = find(pid).children;
        auto& init_children = find(1).children;

        for (auto item : children) {
            init_children.insert(item);
            find(item).ppid = 1;
        }

        children.clear();
    }

    // the process MUST exist, or the behavior is undefined
    void send_signal(pid_t pid, kernel::signal_list::signo_type signal)
    {
        auto& proc = find(pid);
        proc.send_signal(signal);
    }
    void send_signal_grp(pid_t pgid, kernel::signal_list::signo_type signal)
    {
        // TODO: find processes that are in the same session quickly
        for (auto& [ pid, proc ] : m_procs) {
            if (proc.pgid != pgid)
                continue;
            proc.send_signal(signal);
        }
    }

    void kill(pid_t pid, int exit_code);
};

// TODO: lock and unlock
class readyqueue final {
public:
    using thread = kernel::tasks::thread;
    using list_type = std::list<thread*>;

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
    { m_thds.push_back(thd); }

    constexpr thread* pop(void)
    {
        m_thds.remove_if([](thread* item) {
            return !item->is_ready();
        });
        auto* retval = m_thds.front();
        m_thds.pop_front();
        return retval;
    }

    constexpr thread* query(void)
    {
        auto* thd = this->pop();
        this->push(thd);
        return thd;
    }

    constexpr void remove_all(thread* thd)
    { m_thds.remove(thd); }
};

void NORETURN init_scheduler(void);
/// @return true if returned normally, false if being interrupted
bool schedule(void);
void NORETURN schedule_noreturn(void);

constexpr uint32_t push_stack(uint32_t** stack, uint32_t val)
{
    --*stack;
    **stack = val;
    return val;
}

void k_new_thread(void (*func)(void*), void* data);

void NORETURN freeze(void);
void NORETURN kill_current(int signo);

void check_signal(void);
