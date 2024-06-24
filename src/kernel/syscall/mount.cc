#include <errno.h>

#include <types/path.hpp>

#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>

long _syscall_mount(interrupt_stack_normal* data)
{
    SYSCALL_ARG1(const char __user*, source);
    SYSCALL_ARG2(const char __user*, target);
    SYSCALL_ARG3(const char __user*, fstype);
    SYSCALL_ARG4(unsigned long, flags);
    SYSCALL_ARG5(const void __user*, _fsdata);

    if (!fstype)
        return -EINVAL;

    // TODO: use copy_from_user
    auto path = current_process->pwd + target;
    auto* mountpoint = fs::vfs_open(*current_process->root, path);

    if (!mountpoint)
        return -ENOENT;

    return mountpoint->ind->fs->mount(mountpoint, source,
            path.full_path().c_str(), fstype, flags, _fsdata);
}
