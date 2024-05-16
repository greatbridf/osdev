#include <kernel/vfs.hpp>
#include <kernel/mm.hpp>
#include <kernel/log.hpp>

#include <algorithm>
#include <vector>
#include <map>

using fs::vfs, fs::inode, fs::dentry;

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

class tmpfs : public virtual vfs {
private:
    using fe_t = tmpfs_file_entry;
    using vfe_t = std::vector<fe_t>;
    using fdata_t = std::vector<char>;

private:
    std::map<ino_t, void*> inode_data;
    ino_t _next_ino;

private:
    ino_t _assign_ino(void)
    {
        return _next_ino++;
    }

    static constexpr vfe_t* as_vfe(void* data)
    {
        return static_cast<vfe_t*>(data);
    }
    static constexpr fdata_t* as_fdata(void* data)
    {
        return static_cast<fdata_t*>(data);
    }
    static constexpr ptr_t as_val(void* data)
    {
        return std::bit_cast<ptr_t>(data);
    }
    inline void* _getdata(ino_t ino) const
    {
        return inode_data.find(ino)->second;
    }
    inline ino_t _savedata(void* data)
    {
        ino_t ino = _assign_ino();
        inode_data.insert(std::make_pair(ino, data));
        return ino;
    }
    inline ino_t _savedata(ptr_t data)
    {
        return _savedata((void*)data);
    }

protected:
    inline vfe_t* mk_fe_vector() { return new vfe_t{}; }
    inline fdata_t* mk_data_vector() { return new fdata_t{}; }

    void mklink(inode* dir, inode* inode, const char* filename)
    {
        auto* fes = as_vfe(_getdata(dir->ino));
        fes->emplace_back(fe_t {
            .ino = inode->ino,
            .filename = {} });
        dir->size += sizeof(fe_t);

        auto& emplaced = fes->back();

        strncpy(emplaced.filename, filename, sizeof(emplaced.filename));
        emplaced.filename[sizeof(emplaced.filename) - 1] = 0;

        ++inode->nlink;
    }

    virtual int readdir(inode* dir, size_t offset, const fs::vfs::filldir_func& filldir) override
    {
        if (!S_ISDIR(dir->mode)) {
            return -1;
        }

        auto& entries = *as_vfe(_getdata(dir->ino));
        size_t off = offset / sizeof(fe_t);

        size_t nread = 0;

        for (; (off + 1) <= entries.size(); ++off, nread += sizeof(fe_t)) {
            const auto& entry = entries[off];
            auto* ind = get_inode(entry.ino);

            // inode mode filetype is compatible with user dentry filetype
            auto ret = filldir(entry.filename, 0, ind, ind->mode & S_IFMT);
            if (ret != GB_OK)
                break;
        }

        return nread;
    }

public:
    explicit tmpfs(void)
        : _next_ino(1)
    {
        auto& in = *cache_inode(0, _savedata(mk_fe_vector()), S_IFDIR | 0777, 0, 0);

        mklink(&in, &in, ".");
        mklink(&in, &in, "..");

        register_root_node(&in);
    }
    virtual size_t read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override
    {
        if (!S_ISREG(file->mode))
            return 0;

        auto* data = as_fdata(_getdata(file->ino));
        size_t fsize = data->size();

        if (offset + n > fsize)
            n = fsize - offset;

        if (buf_size < n) {
            n = buf_size;
        }

        memcpy(buf, data->data() + offset, n);

        return n;
    }

    virtual size_t write(inode* file, const char* buf, size_t offset, size_t n) override
    {
        if (!S_ISREG(file->mode))
            return 0;

        auto* data = as_fdata(_getdata(file->ino));

        if (data->size() < offset + n)
            data->resize(offset+n);
        memcpy(data->data() + offset, buf, n);

        file->size = data->size();

        return n;
    }

    virtual int inode_mkfile(dentry* dir, const char* filename, mode_t mode) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto& file = *cache_inode(0, _savedata(mk_data_vector()), S_IFREG | mode, 0, 0);
        mklink(dir->ind, &file, filename);

        if (dir->flags.present)
            dir->append(get_inode(file.ino), filename);

        return GB_OK;
    }

    virtual int inode_mknode(dentry* dir, const char* filename, mode_t mode, dev_t dev) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        if (!S_ISBLK(mode) && !S_ISCHR(mode))
            return -EINVAL;

        auto& node = *cache_inode(0, _savedata(dev), mode, 0, 0);
        mklink(dir->ind, &node, filename);

        if (dir->flags.present)
            dir->append(get_inode(node.ino), filename);

        return GB_OK;
    }

    virtual int inode_mkdir(dentry* dir, const char* dirname, mode_t mode) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto new_dir = cache_inode(0, _savedata(mk_fe_vector()), S_IFDIR | (mode & 0777), 0, 0);
        mklink(new_dir, new_dir, ".");

        mklink(dir->ind, new_dir, dirname);
        mklink(new_dir, dir->ind, "..");

        if (dir->flags.present)
            dir->append(new_dir, dirname);

        return GB_OK;
    }

    virtual int symlink(dentry* dir, const char* linkname, const char* target) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* data = mk_data_vector();
        data->resize(strlen(target));
        memcpy(data->data(), target, data->size());

        auto& file = *cache_inode(data->size(), _savedata(data), S_IFLNK | 0777, 0, 0);
        mklink(dir->ind, &file, linkname);

        if (dir->flags.present)
            dir->append(get_inode(file.ino), linkname);

        return 0;
    }

    virtual int readlink(inode* file, char* buf, size_t buf_size) override
    {
        if (!S_ISLNK(file->mode))
            return -EINVAL;

        auto* data = as_fdata(_getdata(file->ino));
        size_t size = data->size();

        size = std::min(size, buf_size);

        memcpy(buf, data->data(), size);

        return size;
    }

    virtual int inode_statx(dentry* dent, statx* st, unsigned int mask) override
    {
        auto* ind = dent->ind;
        const mode_t mode = ind->mode;

        st->stx_mask = 0;

        if (mask & STATX_NLINK) {
            st->stx_nlink = ind->nlink;
            st->stx_mask |= STATX_NLINK;
        }

        // TODO: set modification time
        if (mask & STATX_MTIME) {
            st->stx_mtime = {};
            st->stx_mask |= STATX_MTIME;
        }

        if (mask & STATX_SIZE) {
            st->stx_size = ind->size;
            st->stx_mask |= STATX_SIZE;
        }

        st->stx_mode = 0;
        if (mask & STATX_MODE) {
            st->stx_mode |= ind->mode & ~S_IFMT;
            st->stx_mask |= STATX_MODE;
        }

        if (mask & STATX_TYPE) {
            st->stx_mode |= ind->mode & S_IFMT;
            if (S_ISBLK(mode) || S_ISCHR(mode)) {
                auto nd = (dev_t)as_val(_getdata(ind->ino));
                st->stx_rdev_major = NODE_MAJOR(nd);
                st->stx_rdev_minor = NODE_MINOR(nd);
            }
            st->stx_mask |= STATX_TYPE;
        }

        if (mask & STATX_INO) {
            st->stx_ino = ind->ino;
            st->stx_mask |= STATX_INO;
        }

        if (mask & STATX_BLOCKS) {
            st->stx_blocks = align_up<9>(ind->size) / 512;
            st->stx_blksize = 4096;
            st->stx_mask |= STATX_BLOCKS;
        }

        if (mask & STATX_UID) {
            st->stx_uid = ind->uid;
            st->stx_mask |= STATX_UID;
        }

        if (mask & STATX_GID) {
            st->stx_gid = ind->gid;
            st->stx_mask |= STATX_GID;
        }

        return GB_OK;
    }

    virtual int inode_rmfile(dentry* dir, const char* filename) override
    {
        if (!dir->flags.dir)
            return -ENOTDIR;

        auto* vfe = as_vfe(_getdata(dir->ind->ino));
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
                auto* filedata = as_fdata(_getdata(iter->ino));
                assert(filedata);

                delete filedata;
            }

            free_inode(iter->ino);
            dir->remove(filename);

            vfe->erase(iter);

            return 0;
        }

        kmsg("[tmpfs] warning: file entry not found in vfe\n");
        return -EIO;
    }

    virtual int dev_id(inode* file, dev_t& out_dev) override
    {
        out_dev = as_val(_getdata(file->ino));
        return 0;
    }

    virtual int truncate(inode* file, size_t size) override
    {
        if (!S_ISREG(file->mode))
            return -EINVAL;

        auto* data = as_fdata(_getdata(file->ino));
        data->resize(size);
        file->size = size;
        return GB_OK;
    }
};

static tmpfs* create_tmpfs(dev_t)
{
    return new tmpfs;
}

int fs::register_tmpfs()
{
    fs::register_fs("tmpfs", {create_tmpfs});
    return 0;
}
