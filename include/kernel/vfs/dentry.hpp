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
extern "C" int d_path(struct dentry* dentry, struct dentry* root,
                      char* out_path, size_t buflen);
dentry_pointer d_get(const dentry_pointer& dp);

} // namespace fs
