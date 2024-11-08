#pragma once

#include <string>

#include <bits/alltypes.h>

#include <types/path.hpp>

#include <kernel/async/lock.hpp>

struct dentry;

namespace fs {

struct rust_vfs_handle {
    void* data[2];
};

struct dentry_deleter {
    void operator()(struct dentry* dentry) const;
};

using dentry_pointer = std::unique_ptr<struct dentry, dentry_deleter>;
dentry_pointer d_get(const dentry_pointer& dp);

} // namespace fs
