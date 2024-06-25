#pragma once

#include <list>
#include <map>
#include <memory>
#include <queue>
#include <set>
#include <tuple>
#include <utility>

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <sys/types.h>

#include <kernel/task/current.hpp>
#include <kernel/task/thread.hpp>

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/path.hpp>
#include <types/types.h>

#include <kernel/async/waitlist.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/signal.hpp>
#include <kernel/tty.hpp>
#include <kernel/user/thread_local.hpp>
#include <kernel/vfs.hpp>

class process;

class proclist;

inline process* volatile current_process;
inline proclist* procs;

struct process_attr {
    uint16_t system : 1;
    uint16_t zombie : 1 = 0;
};

struct thread_attr {
    uint32_t system : 1;
    uint32_t ready : 1;
};

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
                    .append = 0,
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
                    .append = 0,
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
            if (!(iter->second.flags & FD_CLOEXEC)) {
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
    kernel::mem::mm_list mms {};
    std::set<kernel::task::thread> thds;
    kernel::async::wait_list waitlist;

    kernel::async::mutex mtx_waitprocs;
    std::list<wait_obj> waitprocs;

    process_attr attr {};
    filearr files;
    types::path pwd;
    mode_t umask { 0022 };

    pid_t pid {};
    pid_t ppid {};
    pid_t pgid {};
    pid_t sid {};

    kernel::tty::tty* control_tty {};
    fs::dentry* root { fs::fs_root };
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
private:
    std::map<pid_t, process> m_procs;
    pid_t m_nextpid = 2;

    constexpr pid_t next_pid() { return m_nextpid++; }
    process& real_emplace(pid_t pid, pid_t ppid);

public:
    proclist();

    constexpr process& copy_from(process& proc)
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

    constexpr std::pair<process*, bool> try_find(pid_t pid) const
    {
        auto iter = m_procs.find(pid);
        if (iter)
            return { (process*)&iter->second, true };
        else
            return { nullptr, false };
    }

    // if process doesn't exist, the behavior is undefined
    constexpr process& find(pid_t pid)
    {
        auto [ ptr, found] = try_find(pid);
        assert(found);
        return *ptr;
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

void NORETURN init_scheduler(kernel::mem::paging::pfn_t kernel_stack_pfn);
/// @return true if returned normally, false if being interrupted
bool schedule(void);
void NORETURN schedule_noreturn(void);

void k_new_thread(void (*func)(void*), void* data);

void NORETURN freeze(void);
void NORETURN kill_current(int signo);

void check_signal(void);
