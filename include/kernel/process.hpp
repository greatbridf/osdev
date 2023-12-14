#pragma once

#include <map>
#include <list>
#include <queue>
#include <set>
#include <tuple>
#include <utility>

#include <fcntl.h>
#include <kernel/errno.h>
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
    uint32_t wait : 1;
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

    int* __user set_child_tid {};
    int* __user clear_child_tid {};

    explicit inline thread(pid_t owner)
        : owner { owner }
        , attr { .system = 1, .ready = 1, .wait = 0, }
    {
        alloc_kstack();
    }

    inline thread(const thread& val, pid_t owner)
        : owner { owner } , attr { val.attr }
    {
        alloc_kstack();
    }

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
public:
    using container_type = std::list<fs::file>;
    using array_type = std::map<int, container_type::iterator>;

private:
    inline static container_type* files;
    array_type arr;
    std::priority_queue<int, std::vector<int>, std::greater<int>> _fds;
    int _greatest_fd;

public:
    inline static void init_global_file_container(void)
    {
        files = new container_type;
    }

private:
    // iter should not be nullptr
    constexpr void _close(container_type::iterator iter)
    {
        if (iter->ref == 1) {
            if (iter->type == fs::file::types::pipe) {
                assert(iter->flags.read | iter->flags.write);
                if (iter->flags.read)
                    iter->ptr.pp->close_read();
                else
                    iter->ptr.pp->close_write();

                if (iter->ptr.pp->is_free())
                    delete iter->ptr.pp;
            }

            files->erase(iter);
        } else
            --iter->ref;
    }

    constexpr int next_fd()
    {
        if (_fds.empty())
            return _greatest_fd++;
        int retval = _fds.top();
        _fds.pop();
        return retval;
    }

public:
    constexpr filearr(const filearr&) = delete;
    constexpr filearr& operator=(const filearr&) = delete;
    constexpr filearr& operator=(filearr&&) = delete;
    constexpr filearr(void) = default;
    constexpr filearr(filearr&& val) = default;

    constexpr int dup(int old_fd)
    {
        return dup2(old_fd, next_fd());
    }

    // TODO: the third parameter should be int flags
    //       determining whether the fd should be closed
    //       after exec() (FD_CLOEXEC)
    constexpr int dup2(int old_fd, int new_fd)
    {
        close(new_fd);

        auto iter = arr.find(old_fd);
        if (!iter)
            return -EBADF;

        auto [ _, iter_file ] = *iter;

        this->arr.emplace(new_fd, iter_file);
        ++iter_file->ref;
        return new_fd;
    }

    constexpr void dup_all(const filearr& orig)
    {
        this->_fds = orig._fds;
        this->_greatest_fd = orig._greatest_fd;
        for (auto [ fd, iter_file ] : orig.arr) {
            this->arr.emplace(fd, iter_file);
            ++iter_file->ref;
        }
    }

    constexpr fs::file* operator[](int i) const
    {
        auto iter = arr.find(i);
        if (!iter)
            return nullptr;
        return &iter->second;
    }

    int pipe(int pipefd[2])
    {
        // TODO: set read/write flags
        auto* pipe = new fs::pipe;

        auto iter = files->emplace(files->cend(), fs::file {
            fs::file::types::pipe,
            { .pp = pipe },
            nullptr,
            0,
            1,
            {
                .read = 1,
                .write = 0,
            },
        });

        bool inserted = false;
        int fd = next_fd();
        std::tie(std::ignore, inserted) = arr.emplace(fd, iter);
        assert(inserted);

        // TODO: use copy_to_user()
        pipefd[0] = fd;

        iter = files->emplace(files->cend(), fs::file {
            fs::file::types::pipe,
            { .pp = pipe },
            nullptr,
            0,
            1,
            {
                .read = 0,
                .write = 1,
            },
        });
        fd = next_fd();
        std::tie(std::ignore, inserted) = arr.emplace(fd, iter);
        assert(inserted);

        // TODO: use copy_to_user()
        pipefd[1] = fd;

        return 0;
    }

    int open(const process& current, const char* filename, uint32_t flags);

    constexpr void close(int fd)
    {
        auto iter = arr.find(fd);
        if (!iter)
            return;

        _close(iter->second);
        _fds.push(fd);
        arr.erase(iter);
    }

    constexpr void close_all(void)
    {
        for (auto&& [ fd, file ] : arr) {
            _close(file);
            _fds.push(fd);
        }
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
    mutable kernel::mm_list mms;
    std::set<kernel::tasks::thread> thds;
    kernel::cond_var cv_wait;
    std::list<wait_obj> waitlist;
    process_attr attr {};
    filearr files;
    types::string<> pwd;
    kernel::signal_list signals;

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

    constexpr bool has_child(pid_t pid)
    {
        auto& proc = find(pid);
        return !proc.children.empty();
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
    void send_signal(pid_t pid, kernel::sig_t signal)
    {
        auto& proc = this->find(pid);
        proc.signals.set(signal);
    }
    void send_signal_grp(pid_t pgid, kernel::sig_t signal)
    {
        for (auto& [ pid, proc ] : m_procs) {
            if (proc.pgid == pgid)
                proc.signals.set(signal);
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
            return !item->attr.ready;
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
void NORETURN kill_current(int exit_code);

void check_signal(void);
