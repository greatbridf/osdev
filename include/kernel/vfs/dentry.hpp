#pragma once

#include <list>

#include <types/string.hpp>
#include <types/hash_map.hpp>
#include <types/path.hpp>

#include <kernel/vfs/inode.hpp>

namespace fs {

struct dentry {
public:
    using name_type = types::string<>;

private:
    std::list<dentry>* children = nullptr;
    types::hash_map<name_type, dentry*>* idx_children = nullptr;

public:
    dentry* parent;
    inode* ind;
    struct {
        uint32_t dir : 1; // whether the dentry is a directory.
        // if dir is 1, whether children contains valid data.
        // otherwise, ignored
        uint32_t present : 1;
    } flags;
    name_type name;

    explicit dentry(dentry* parent, inode* ind, name_type name);
    dentry(const dentry& val) = delete;
    constexpr dentry(dentry&& val)
        : children(std::exchange(val.children, nullptr))
        , idx_children(std::exchange(val.idx_children, nullptr))
        , parent(std::exchange(val.parent, nullptr))
        , ind(std::exchange(val.ind, nullptr))
        , flags { val.flags }
        , name(std::move(val.name))
    {
        if (children) {
            for (auto& item : *children)
                item.parent = this;
        }
    }

    dentry& operator=(const dentry& val) = delete;
    dentry& operator=(dentry&& val) = delete;

    constexpr ~dentry()
    {
        if (children) {
            delete children;
            children = nullptr;
        }
        if (idx_children) {
            delete idx_children;
            idx_children = nullptr;
        }
    }

    int load();

    dentry* append(inode* ind, name_type name);

    dentry* find(const name_type& name);

    dentry* replace(dentry* val);

    void remove(const name_type& name);

    // out_dst SHOULD be empty
    void path(const dentry& root, types::path& out_dst) const;
};

} // namespace fs
