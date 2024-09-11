#pragma once

#include <list>
#include <string>

#include <types/hash.hpp>
#include <types/path.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/vfs/inode.hpp>

namespace fs {
static constexpr unsigned long D_PRESENT = 1 << 0;
static constexpr unsigned long D_DIRECTORY = 1 << 1;
static constexpr unsigned long D_LOADED = 1 << 2;
static constexpr unsigned long D_MOUNTPOINT = 1 << 3;

struct dentry {
    struct dcache* cache;
    vfs* fs;

    struct inode* inode;
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
