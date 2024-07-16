#include <map>

#include <errno.h>
#include <stdint.h>
#include <sys/mount.h>
#include <unistd.h>

#include <kernel/hw/timer.hpp>
#include <kernel/module.hpp>
#include <kernel/process.hpp>
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

static ssize_t schedstat_read(char* page, size_t n)
{
    auto orig_n = n;

    if (n == 0)
        return n;

    int nw = snprintf(page, n, "%d\n", kernel::hw::timer::current_ticks());
    n -= nw, page += nw;

    for (const auto& proc : *procs) {
        for (const auto& thd : proc.second.thds) {
            int nwrote = snprintf(page, n, "%d %x %d\n", proc.first, thd.tid(), thd.elected_times);

            n -= nwrote;
            page += nwrote;
        }
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

        auto* ind = alloc_inode(ino);
        ind->mode = S_IFREG | 0666;

        return inserted ? 0 : -EEXIST;
    }

    procfs(const char* _source)
        : vfs(make_device(0, 10), 4096)
        , source{_source}
    {
        auto* ind = alloc_inode(0);
        ind->mode = S_IFDIR | 0777;

        create_file("mounts", mounts_read, nullptr);
        create_file("schedstat", schedstat_read, nullptr);

        register_root_node(ind);
    }

    ssize_t read(inode* file, char* buf, size_t buf_size, size_t n, off_t offset) override
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

    ssize_t readdir(inode *dir, size_t offset, const filldir_func &callback) override
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
            if (ret != 0)
                return -EIO;
            ++nread;
        }

        return nread;
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
