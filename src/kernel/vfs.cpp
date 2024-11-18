#include <cstddef>

#include <assert.h>
#include <bits/alltypes.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mount.h>
#include <sys/types.h>

#include <types/allocator.hpp>
#include <types/path.hpp>

#include <kernel/log.hpp>
#include <kernel/process.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>

static fs::chrdev_ops** chrdevs[256];

int fs::register_char_device(dev_t node, const fs::chrdev_ops& ops) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major])
        chrdevs[major] = new chrdev_ops* [256] {};

    if (chrdevs[major][minor])
        return -EEXIST;

    chrdevs[major][minor] = new chrdev_ops{ops};
    return 0;
}

ssize_t fs::char_device_read(dev_t node, char* buf, size_t buf_size, size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& read = chrdevs[major][minor]->read;
    if (!read)
        return -EINVAL;

    return read(buf, buf_size, n);
}

ssize_t fs::char_device_write(dev_t node, const char* buf, size_t n) {
    int major = NODE_MAJOR(node);
    int minor = NODE_MINOR(node);

    if (!chrdevs[major] || !chrdevs[major][minor])
        return -EINVAL;

    auto& write = chrdevs[major][minor]->write;
    if (!write)
        return -EINVAL;

    return write(buf, n);
}

extern "C" void r_dput(struct dentry* dentry);
extern "C" struct dentry* r_dget(struct dentry* dentry);

void fs::dentry_deleter::operator()(struct dentry* dentry) const {
    if (dentry)
        r_dput(dentry);
}

fs::dentry_pointer fs::d_get(const dentry_pointer& dp) {
    if (!dp)
        return nullptr;

    return dentry_pointer{r_dget(dp.get())};
}
