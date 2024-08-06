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
    auto [ mountpoint, status ] = current_open(target);
    if (!mountpoint || status)
        return status;

    return fs::mount(mountpoint.get(), source, target, fstype, flags, _fsdata);
}
