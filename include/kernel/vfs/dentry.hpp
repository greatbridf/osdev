#pragma once

#include <string>

#include <bits/alltypes.h>

#include <types/hash.hpp>
#include <types/path.hpp>

#include <kernel/async/lock.hpp>

namespace fs {
static constexpr unsigned long D_PRESENT = 1 << 0;
static constexpr unsigned long D_DIRECTORY = 1 << 1;
static constexpr unsigned long D_LOADED = 1 << 2;
static constexpr unsigned long D_MOUNTPOINT = 1 << 3;
static constexpr unsigned long D_SYMLINK = 1 << 4;

struct rust_vfs_handle {
    void* data[2];
};

struct rust_inode_handle {
    void* data[2];
};

struct inode_data {
    uint64_t ino;
    uint64_t size;
    uint64_t nlink;

    struct timespec atime;
    struct timespec mtime;
    struct timespec ctime;

    uint32_t uid;
    uint32_t gid;
    uint32_t mode;
};

struct dentry {
    struct rust_vfs_handle fs;
    struct rust_inode_handle inode;

    struct dcache* cache;
    struct dentry* parent;

    // list head
    struct dentry* prev;
    struct dentry* next;

    unsigned long flags;
    types::hash_t hash;

    // TODO: use atomic
    std::size_t refcount;

    std::string name;
};

struct dentry_deleter {
    void operator()(struct dentry* dentry) const;
};

using dentry_pointer = std::unique_ptr<struct dentry, dentry_deleter>;

struct dcache {
    struct dentry** arr;
    int hash_bits;

    std::size_t size;
};

std::pair<struct dentry*, int> d_find(struct dentry* parent,
                                      types::string_view name);
std::string d_path(const struct dentry* dentry, const struct dentry* root);

dentry_pointer d_get(const dentry_pointer& dp);
struct dentry* d_get(struct dentry* dentry);
struct dentry* d_put(struct dentry* dentry);

void dcache_init(struct dcache* cache, int hash_bits);
void dcache_drop(struct dcache* cache);

struct dentry* dcache_alloc(struct dcache* cache);
void dcache_init_root(struct dcache* cache, struct dentry* root);

} // namespace fs

struct rust_get_cxx_string_result {
    const char* data;
    size_t len;
};

void rust_get_cxx_string(const std::string* str,
                         rust_get_cxx_string_result* out_result);
void rust_operator_eql_cxx_string(const std::string* str, std::string* dst);
