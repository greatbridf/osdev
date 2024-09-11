#pragma once

#include <list>
#include <map>
#include <memory>
#include <queue>
#include <set>
#include <tuple>
#include <utility>

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <sys/types.h>

#include <types/allocator.hpp>
#include <types/cplusplus.hpp>
#include <types/path.hpp>
#include <types/types.h>

#include <kernel/async/waitlist.hpp>
#include <kernel/interrupt.hpp>
#include <kernel/mem/mm_list.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/signal.hpp>
#include <kernel/task/current.hpp>
#include <kernel/task/thread.hpp>
#include <kernel/tty.hpp>
#include <kernel/user/thread_local.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/filearr.hpp>

class process;

class proclist;

inline process* volatile current_process;
inline proclist* procs;

struct process_attr {
    uint16_t system : 1;
    uint16_t zombie : 1 = 0;
};

class process {
   public:
    struct wait_obj {
        pid_t pid;
        int code;
    };

   public:
    kernel::mem::mm_list mms{};
    std::set<kernel::task::thread> thds;
    kernel::async::wait_list waitlist;

    kernel::async::mutex mtx_waitprocs;
    std::list<wait_obj> waitprocs;

    process_attr attr{};
    fs::filearray files;
    fs::dentry_pointer cwd{};
    mode_t umask{0022};

    pid_t pid{};
    pid_t ppid{};
    pid_t pgid{};
    pid_t sid{};

    kernel::tty::tty* control_tty{};
    struct fs::fs_context fs_context;
    std::set<pid_t> children;

   public:
    process(const process&) = delete;
    explicit process(const process& parent, pid_t pid);

    // this function is used for system initialization
    // DO NOT use this after the system is on
    explicit process(pid_t pid, pid_t ppid);

    constexpr bool is_system(void) const { return attr.system; }
    constexpr bool is_zombie(void) const { return attr.zombie; }

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

    constexpr process& copy_from(process& proc) {
        pid_t pid = next_pid();
        auto [iter, inserted] = m_procs.try_emplace(pid, proc, pid);
        assert(inserted);

        proc.children.insert(pid);
        return iter->second;
    }

    constexpr void remove(pid_t pid) {
        make_children_orphans(pid);

        auto proc_iter = m_procs.find(pid);

        auto ppid = proc_iter->second.ppid;
        find(ppid).children.erase(pid);

        m_procs.erase(proc_iter);
    }

    constexpr std::pair<process*, bool> try_find(pid_t pid) const {
        auto iter = m_procs.find(pid);
        if (iter)
            return {(process*)&iter->second, true};
        else
            return {nullptr, false};
    }

    // if process doesn't exist, the behavior is undefined
    constexpr process& find(pid_t pid) {
        auto [ptr, found] = try_find(pid);
        assert(found);
        return *ptr;
    }

    constexpr void make_children_orphans(pid_t pid) {
        auto& children = find(pid).children;
        auto& init_children = find(1).children;

        for (auto item : children) {
            init_children.insert(item);
            find(item).ppid = 1;
        }

        children.clear();
    }

    // the process MUST exist, or the behavior is undefined
    void send_signal(pid_t pid, kernel::signal_list::signo_type signal) {
        auto& proc = find(pid);
        proc.send_signal(signal);
    }
    void send_signal_grp(pid_t pgid, kernel::signal_list::signo_type signal) {
        // TODO: find processes that are in the same session quickly
        for (auto& [pid, proc] : m_procs) {
            if (proc.pgid != pgid)
                continue;
            proc.send_signal(signal);
        }
    }

    void kill(pid_t pid, int exit_code);

    constexpr auto begin() const { return m_procs.begin(); }
    constexpr auto end() const { return m_procs.end(); }
};

void NORETURN init_scheduler(kernel::mem::paging::pfn_t kernel_stack_pfn);
/// @return true if returned normally, false if being interrupted
bool schedule(void);
void NORETURN schedule_noreturn(void);

void NORETURN freeze(void);
void NORETURN kill_current(int signo);

void check_signal(void);
