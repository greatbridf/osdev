#include <errno.h>

#include <types/path.hpp>

#include <kernel/process.hpp>
#include <kernel/syscall.hpp>
#include <kernel/vfs.hpp>

int _syscall_symlink(interrupt_stack* data)
{
    SYSCALL_ARG1(const char __user*, target);
    SYSCALL_ARG2(const char __user*, linkpath);

    // TODO: use copy_from_user
    auto path = current_process->pwd + linkpath;
    auto* dent = fs::vfs_open(*current_process->root, path);

    if (dent)
        return -EEXIST;

    auto linkname = path.last_name();
    path.remove_last();

    dent = fs::vfs_open(*current_process->root, path);
    if (!dent)
        return -ENOENT;

    return dent->ind->fs->symlink(dent, linkname.c_str(), target);
}

int _syscall_readlink(interrupt_stack* data)
{
    SYSCALL_ARG1(const char __user*, pathname);
    SYSCALL_ARG2(char __user*, buf);
    SYSCALL_ARG3(size_t, buf_size);

    // TODO: use copy_from_user
    auto path = current_process->pwd + pathname;
    auto* dent = fs::vfs_open(*current_process->root, path, false);

    if (!dent)
        return -ENOENT;

    if (buf_size <= 0 || !S_ISLNK(dent->ind->mode))
        return -EINVAL;

    // TODO: use copy_to_user
    return dent->ind->fs->readlink(dent->ind, buf, buf_size);
}
