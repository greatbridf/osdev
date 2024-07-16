#include <algorithm>
#include <map>
#include <vector>

#include <stdint.h>

#include <kernel/log.hpp>
#include <kernel/vfs.hpp>

using namespace fs;

struct tmpfs_file_entry {
    ino_t ino;
    std::string filename;
};

class tmpfs : public virtual vfs {
private:
    using fe_t = tmpfs_file_entry;
    using vfe_t = std::vector<fe_t>;
    using fdata_t = std::vector<char>;

private:
    ino_t m_next_ino;

private:
    ino_t assign_ino()
    {
        return m_next_ino++;
    }

protected:
    inline vfe_t* make_vfe() { return new vfe_t{}; }
    inline fdata_t* make_fdata() { return new fdata_t{}; }

    void mklink(inode* dir, inode* ind, const char* filename)
    {
        auto& fes = *(vfe_t*)dir->fs_data;
        fes.emplace_back(fe_t { ind->ino, filename });

        dir->size += sizeof(fe_t);
        ++ind->nlink;
    }

    virtual ssize_t readdir(inode* dir, size_t offset, const vfs::filldir_func& filldir) override
    {
        if (!S_ISDIR(dir->mode))
            return -ENOTDIR;

        auto& entries = *(vfe_t*)dir->fs_data;
        size_t off = offset / sizeof(fe_t);

        size_t nread = 0;

        for (; (off + 1) <= entries.size(); ++off, nread += sizeof(fe_t)) {
            const auto& entry = entries[off];
            auto* ind = get_inode(entry.ino);

            // inode mode filetype is compatible with user dentry filetype
            auto ret = filldir(entry.filename.c_str(), 0, ind, ind->mode & S_IFMT);
            if (ret != 0)
                break;
        }

        return nread;
    }

public:
    explicit tmpfs()
        : vfs(make_device(0, 2), 4096)
        , m_next_ino{1}
    {
        auto* in = alloc_inode(assign_ino());

        in->fs_data = make_vfe();
        in->mode = S_IFDIR | 0777;

        mklink(in, in, ".");
        mklink(in, in, "..");

        register_root_node(in);
    }

    virtual ssize_t read(inode* file, char* buf, size_t buf_size, size_t count, off_t offset) override
    {
        if (!S_ISREG(file->mode))
            return -EINVAL;

        auto* data = (fdata_t*)file->fs_data;
        size_t fsize = data->size();

        if (offset + count > fsize)
            count = fsize - offset;

        if (buf_size < count) {
            count = buf_size;
        }

        memcpy(buf, data->data() + offset, count);

        return count;
    }

    virtual ssize_t write(inode* file, const char* buf, size_t count, off_t offset) override
    {
        if (!S_ISREG(file->mode))
            return -EINVAL;

        auto* data = (fdata_t*)file->fs_data;

        if (data->size() < offset + count)
            data->resize(offset+count);
        memcpy(data->data() + offset, buf, count);

        file->size = data->size();

        return count;
    }

    virtual int inode_mkfile(dentry* dir, const char* filename, mode_t mode) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* file = alloc_inode(assign_ino());
        file->mode = S_IFREG | (mode & 0777);
        file->fs_data = make_fdata();

        mklink(dir->ind, file, filename);

        if (dir->flags.present)
            dir->append(file, filename);

        return 0;
    }

    virtual int inode_mknode(dentry* dir, const char* filename, mode_t mode, dev_t dev) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        if (!S_ISBLK(mode) && !S_ISCHR(mode))
            return -EINVAL;

        if (dev & ~0xffff)
            return -EINVAL;

        auto* node = alloc_inode(assign_ino());
        node->mode = mode;
        node->fs_data = (void*)(uintptr_t)dev;

        mklink(dir->ind, node, filename);

        if (dir->flags.present)
            dir->append(node, filename);

        return 0;
    }

    virtual int inode_mkdir(dentry* dir, const char* dirname, mode_t mode) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* new_dir = alloc_inode(assign_ino());
        new_dir->mode = S_IFDIR | (mode & 0777);
        new_dir->fs_data = make_vfe();

        mklink(new_dir, new_dir, ".");

        mklink(dir->ind, new_dir, dirname);
        mklink(new_dir, dir->ind, "..");

        if (dir->flags.present)
            dir->append(new_dir, dirname);

        return 0;
    }

    virtual int symlink(dentry* dir, const char* linkname, const char* target) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* data = make_fdata();
        data->resize(strlen(target));
        memcpy(data->data(), target, data->size());

        auto* file = alloc_inode(assign_ino());
        file->mode = S_IFLNK | 0777;
        file->fs_data = data;
        file->size = data->size();

        mklink(dir->ind, file, linkname);

        if (dir->flags.present)
            dir->append(file, linkname);

        return 0;
    }

    virtual int readlink(inode* file, char* buf, size_t buf_size) override
    {
        if (!S_ISLNK(file->mode))
            return -EINVAL;

        auto* data = (fdata_t*)file->fs_data;
        size_t size = data->size();

        size = std::min(size, buf_size);

        memcpy(buf, data->data(), size);

        return size;
    }

    virtual int inode_rmfile(dentry* dir, const char* filename) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* vfe = (vfe_t*)dir->ind->fs_data;
        assert(vfe);

        auto* dent = dir->find(filename);
        if (!dent)
            return -ENOENT;

        for (auto iter = vfe->begin(); iter != vfe->end(); ) {
            if (iter->ino != dent->ind->ino) {
                ++iter;
                continue;
            }

            if (S_ISREG(dent->ind->mode)) {
                // since we do not allow hard links in tmpfs, there is no need
                // to check references, we remove the file data directly
                auto* filedata = (fdata_t*)dent->ind->fs_data;
                assert(filedata);

                delete filedata;
            }

            free_inode(iter->ino);
            dir->remove(filename);

            vfe->erase(iter);

            return 0;
        }

        kmsg("[tmpfs] warning: file entry not found in vfe");
        return -EIO;
    }

    virtual dev_t i_device(inode* file) override
    {
        if (file->mode & S_IFMT & (S_IFBLK | S_IFCHR))
            return (dev_t)(uintptr_t)file->fs_data;
        return -ENODEV;
    }

    virtual int truncate(inode* file, size_t size) override
    {
        if (!S_ISREG(file->mode))
            return -EINVAL;

        auto* data = (fdata_t*)file->fs_data;
        data->resize(size);
        file->size = size;
        return 0;
    }
};

static tmpfs* create_tmpfs(const char*, unsigned long, const void*)
{
    // TODO: flags
    return new tmpfs;
}

int fs::register_tmpfs()
{
    fs::register_fs("tmpfs", {create_tmpfs});
    return 0;
}
