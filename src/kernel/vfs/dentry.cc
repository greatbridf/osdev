#include <assert.h>
#include <errno.h>
#include <sys/stat.h>

#include <types/hash.hpp>
#include <types/list.hpp>
#include <types/path.hpp>

#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/vfs.hpp>

using namespace fs;
using types::hash_t, types::hash_str, types::hash_ptr;

static inline struct dentry* __d_parent(struct dentry* dentry) {
    if (dentry->parent)
        return dentry->parent;
    return dentry;
}

static inline bool __d_is_present(struct dentry* dentry) {
    return dentry->flags & D_PRESENT;
}

static inline bool __d_is_dir(struct dentry* dentry) {
    return dentry->flags & D_DIRECTORY;
}

static inline bool __d_is_loaded(struct dentry* dentry) {
    return dentry->flags & D_LOADED;
}

static inline bool __d_equal(struct dentry* dentry, struct dentry* parent,
                             types::string_view name) {
    return dentry->parent == parent && dentry->name == name;
}

static inline hash_t __d_hash(struct dentry* parent, types::string_view name) {
    assert(parent && parent->cache);
    int bits = parent->cache->hash_bits;

    return hash_str(name, bits) ^ hash_ptr(parent, bits);
}

static inline struct dentry*& __d_first(struct dcache* cache, hash_t hash) {
    return cache->arr[hash & ((1 << cache->hash_bits) - 1)];
}

static inline void __d_add(struct dentry* parent, struct dentry* dentry) {
    assert(!dentry->parent);
    assert(parent->refcount && dentry->refcount);

    dentry->parent = d_get(parent);
    dentry->prev = nullptr;
    dentry->next = __d_first(parent->cache, dentry->hash);

    __d_first(parent->cache, dentry->hash) = d_get(dentry);
    parent->cache->size++;
}

static inline struct dentry* __d_find_fast(struct dentry* parent,
                                           types::string_view name) {
    auto* cache = parent->cache;
    assert(cache);

    hash_t hash = __d_hash(parent, name);
    for (struct dentry* dentry = __d_first(cache, hash); dentry;
         dentry = dentry->next) {
        if (!__d_equal(dentry, parent, name))
            continue;

        return d_get(dentry);
    }

    return nullptr;
}

static inline int __d_load(struct dentry* parent) {
    if (__d_is_loaded(parent))
        return 0;

    auto* inode = parent->inode;
    assert(inode);

    if (!__d_is_dir(parent))
        return -ENOTDIR;
    assert(S_ISDIR(inode->mode));

    auto* fs = parent->inode->fs;
    assert(fs);

    size_t offset = 0;
    while (true) {
        ssize_t off = fs->readdir(
            inode, offset,
            [parent](const char* fn, struct inode* inode, u8) -> int {
                struct dentry* dentry = dcache_alloc(parent->cache);

                dentry->fs = inode->fs;
                dentry->inode = inode;
                dentry->name = fn;

                if (S_ISDIR(inode->mode))
                    dentry->flags = D_PRESENT | D_DIRECTORY;
                else
                    dentry->flags = D_PRESENT;

                dentry->hash = __d_hash(parent, dentry->name);

                __d_add(parent, dentry);

                d_put(dentry);
                return 0;
            });

        if (off == 0)
            break;

        offset += off;
    }

    parent->flags |= D_LOADED;
    return 0;
}

std::pair<struct dentry*, int> fs::d_find(struct dentry* parent,
                                          types::string_view name) {
    assert(__d_is_present(parent));
    if (!__d_is_dir(parent))
        return {nullptr, -ENOTDIR};

    constexpr types::string_view dot{".", 1};
    constexpr types::string_view dotdot{"..", 2};

    if (name == dot)
        return {d_get(parent), 0};

    if (name == dotdot)
        return {d_get(__d_parent(parent)), 0};

    if (!__d_is_loaded(parent)) {
        if (int ret = __d_load(parent); ret != 0)
            return {nullptr, ret};
    }

    struct dentry* ret = __d_find_fast(parent, name);
    if (!ret) {
        auto* dentry = dcache_alloc(parent->cache);
        dentry->fs = parent->fs;

        dentry->name.assign(name.data(), name.size());
        dentry->hash = __d_hash(parent, dentry->name);

        __d_add(parent, dentry);

        return {dentry, -ENOENT};
    }

    return {ret, 0};
}

std::string fs::d_path(const struct dentry* dentry, const struct dentry* root) {
    const struct dentry* dents[32];
    int cnt = 0;

    const struct dentry* cur = dentry;
    while (cur != root) {
        assert(cur && cnt < 32);
        dents[cnt++] = cur;
        cur = cur->parent;
    }

    std::string ret = "/";
    for (int i = cnt - 1; i >= 0; --i) {
        ret += dents[i]->name;
        ret += '/';
    }

    return ret;
}

dentry_pointer fs::d_get(const dentry_pointer& dentry) {
    return d_get(dentry.get());
}

struct dentry* fs::d_get(struct dentry* dentry) {
    assert(dentry);
    ++dentry->refcount;
    return dentry;
}

struct dentry* fs::d_put(struct dentry* dentry) {
    assert(dentry);

    // TODO: if refcount is zero, mark dentry as unused
    --dentry->refcount;
    return dentry;
    ;
}

void dentry_deleter::operator()(struct dentry* dentry) const {
    fs::d_put(dentry);
}

void fs::dcache_init(struct dcache* cache, int hash_bits) {
    cache->hash_bits = hash_bits;
    cache->arr = new struct dentry*[1 << hash_bits]();
    cache->size = 0;
}

void fs::dcache_drop(struct dcache* cache) {
    assert(cache->size == 0);
    delete[] cache->arr;
}

struct dentry* fs::dcache_alloc(struct dcache* cache) {
    struct dentry* dentry = new struct dentry();
    dentry->cache = cache;

    return d_get(dentry);
}

void fs::dcache_init_root(struct dcache* cache, struct dentry* root) {
    assert(cache->size == 0);

    root->prev = root->next = nullptr;
    __d_first(cache, root->hash) = d_get(root);

    cache->size++;
}
