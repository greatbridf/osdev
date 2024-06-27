#include <errno.h>

#include <types/path.hpp>

#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>

int kernel::syscall::do_mount(
        const char __user* source,
        const char __user* target,
        const char __user* fstype,
        unsigned long flags,
        const void __user* _fsdata)
{
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
