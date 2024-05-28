#include <map>

#include <errno.h>
#include <stdint.h>
#include <sys/mount.h>
#include <unistd.h>

#include <types/status.h>

#include <kernel/module.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/vfs.hpp>

struct mount_flags_opt {
    unsigned long flag;
    const char* name;
};

static struct mount_flags_opt mount_opts[] = {
    {MS_NOSUID, ",nosuid"},
    {MS_NODEV, ",nodev"},
    {MS_NOEXEC, ",noexec"},
    {MS_NOATIME, ",noatime"},
    {MS_RELATIME, ",relatime"},
    {MS_LAZYTIME, ",lazytime"},
};

static std::string get_mount_opts(unsigned long mnt_flags)
{
    std::string retval;

    if (mnt_flags & MS_RDONLY)
        retval += "ro";
    else
        retval += "rw";

    for (const auto& opt : mount_opts) {
        if (mnt_flags & opt.flag)
            retval += opt.name;
    }

    return retval;
}

static ssize_t mounts_read(char* page, size_t n)
{
    auto orig_n = n;

    for (const auto& [ _, mdata ] : fs::mounts) {
        if (n == 0)
            break;

        // TODO: get vfs options
        auto mount_flags = get_mount_opts(mdata.flags);

        int nwrote = snprintf(page, n, "%s %s %s %s 0 0\n",
                mdata.source.c_str(), mdata.mount_point.c_str(),
                mdata.fstype.c_str(), mount_flags.c_str());

        n -= nwrote;
        page += nwrote;
    }

    return orig_n - n;
}

namespace fs::proc {

struct proc_file {
    std::string name;

    ssize_t (*read)(char* page_buffer, size_t n);
    ssize_t (*write)(const char* data, size_t n);
};

class procfs : public virtual fs::vfs {
private:
    std::string source;
    std::map<ino_t, proc_file> files;

    ino_t free_ino = 1;

public:
    static procfs* create(const char* source, unsigned long, const void*)
    {
        // TODO: flags
        return new procfs(source);
    }

    int create_file(std::string name,
            ssize_t (*read_func)(char*, size_t),
            ssize_t (*write_func)(const char*, size_t))
    {
        auto ino = free_ino++;

        auto [ _, inserted ] =
            files.insert({ino, proc_file {name, read_func, write_func}});

        cache_inode(0, ino, S_IFREG | 0666, 0, 0);

        return inserted ? 0 : -EEXIST;
    }

    procfs(const char* _source)
        : source{_source}
    {
        auto* ind = cache_inode(0, 0, S_IFDIR | 0777, 0, 0);

        create_file("mounts", mounts_read, nullptr);

        register_root_node(ind);
    }

    size_t read(inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override
    {
        if (file->ino == 0)
            return -EISDIR;

        auto iter = files.find(file->ino);
        if (!iter)
            return -EIO;

        auto& [ ino, pf ] = *iter;

        if (!pf.read)
            return -EINVAL;

        // TODO: fix it
        if (offset)
            return 0;

        // TODO: allocate page buffer
        char* page_buffer = new char[4096];

        ssize_t nread = pf.read(page_buffer, 4096);
        if (nread < 0) {
            delete[] page_buffer;
            return nread;
        }

        n = std::min(n, buf_size);
        n = std::min(n, (size_t)nread);

        strncpy(buf, page_buffer, n);

        delete[] page_buffer;
        return n;
    }

    int readdir(inode *dir, size_t offset, const filldir_func &callback) override
    {
        if (dir->ino != 0)
            return -ENOTDIR;

        // TODO: fix it
        if (offset)
            return 0;

        int nread = 0;
        for (const auto& [ ino, pf ] : files) {
            auto* ind = get_inode(ino);
            int ret = callback(pf.name.c_str(), 0, ind, ind->mode);
            if (ret != GB_OK)
                return -EIO;
            ++nread;
        }

        return nread;
    }

    virtual int inode_statx(dentry* dent, statx* st, unsigned int mask) override
    {
        auto* ind = dent->ind;

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
            st->stx_mask |= STATX_TYPE;
        }

        if (mask & STATX_INO) {
            st->stx_ino = ind->ino;
            st->stx_mask |= STATX_INO;
        }

        if (mask & STATX_BLOCKS) {
            st->stx_blocks = 0;
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

        return 0;
    }
};

class procfs_module : public virtual kernel::module::module {
public:
    procfs_module() : module("procfs") { }

    virtual int init() override
    {
        int ret = fs::register_fs("procfs", procfs::create);
        if (ret != 0)
            return kernel::module::MODULE_FAILED;
        return kernel::module::MODULE_SUCCESS;
    }
};

static kernel::module::module* procfs_init()
{
    return new procfs_module;
}

INTERNAL_MODULE(procfs, procfs_init);

} // namespace fs::proc
