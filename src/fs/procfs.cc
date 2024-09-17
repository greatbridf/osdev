#include <map>

#include <errno.h>
#include <stdint.h>
#include <sys/mount.h>
#include <sys/types.h>
#include <unistd.h>

#include <kernel/async/lock.hpp>
#include <kernel/hw/timer.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/module.hpp>
#include <kernel/process.hpp>
#include <kernel/procfs.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/inode.hpp>
#include <kernel/vfs/vfs.hpp>

using namespace kernel::kmod;
using namespace kernel::procfs;
using fs::inode, fs::make_device;

struct mount_flags_opt {
    unsigned long flag;
    const char* name;
};

static struct mount_flags_opt mount_opts[] = {
    {MS_NOSUID, ",nosuid"},     {MS_NODEV, ",nodev"},
    {MS_NOEXEC, ",noexec"},     {MS_NOATIME, ",noatime"},
    {MS_RELATIME, ",relatime"}, {MS_LAZYTIME, ",lazytime"},
};

static std::string get_mount_opts(unsigned long mnt_flags) {
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

static isize mounts_read(u8* page, usize bufsize) {
    auto orig_bufsize = bufsize;

    for (const auto& [_, mdata] : fs::mounts) {
        // TODO: get vfs options
        auto mount_flags = get_mount_opts(mdata.flags);

        isize nwrote = snprintf((char*)page, bufsize, "%s %s %s %s 0 0\n",
                              mdata.source.c_str(), mdata.mount_point.c_str(),
                              mdata.fstype.c_str(), mount_flags.c_str());
        if (nwrote < 0)
            return nwrote;

        assert((usize)nwrote < bufsize);

        bufsize -= nwrote;
        page += nwrote;
    }

    return orig_bufsize - bufsize;
}

static isize schedstat_read(u8* page, usize bufsize) {
    auto orig_bufsize = bufsize;

    int nw = snprintf((char*)page, bufsize, "%d\n",
                      kernel::hw::timer::current_ticks());
    if (nw < 0)
        return nw;

    assert((usize)nw < bufsize);
    bufsize -= nw, page += nw;

    for (const auto& proc : *procs) {
        for (const auto& thd : proc.second.thds) {
            int nwrote = snprintf((char*)page, bufsize, "%d %x %d\n",
                                  proc.first, thd.tid(), thd.elected_times);
            if (nwrote < 0)
                return nwrote;

            assert((usize)nwrote < bufsize);

            bufsize -= nwrote;
            page += nwrote;
        }
    }

    return orig_bufsize - bufsize;
}

namespace kernel::procfs {

static procfs_file* s_root;
static ino_t s_next_ino = 1;

static mode_t _get_mode(const procfs_file& file) {
    if (file.children)
        return S_IFDIR | 0755;

    mode_t mode = S_IFREG;
    if (file.read)
        mode |= 0444;
    if (file.write)
        mode |= 0200;

    return mode;
}

class procfs : public virtual fs::vfs {
   private:
    std::string source;

    procfs(const char* _source)
        : vfs(make_device(0, 10), 4096), source{_source} {
        assert(s_root);

        auto* ind = alloc_inode(s_root->ino);
        ind->fs_data = s_root;
        ind->mode = _get_mode(*s_root);

        register_root_node(ind);
    }

   public:
    static int init() {
        auto children = std::make_unique<std::vector<procfs_file>>();
        auto procfsFile = std::make_unique<procfs_file>();

        procfsFile->name = "[root]";
        procfsFile->ino = 0;
        procfsFile->read = [](u8*, usize) { return -EISDIR; };
        procfsFile->write = [](const u8*, usize) { return -EISDIR; };
        procfsFile->children = children.release();
        s_root = procfsFile.release();

        kernel::procfs::create(s_root, "mounts", mounts_read, nullptr);
        kernel::procfs::create(s_root, "schedstat", schedstat_read, nullptr);

        return 0;
    }

    static procfs* create(const char* source, unsigned long, const void*) {
        return new procfs(source);
    }

    ssize_t read(inode* file, char* buf, size_t buf_size, size_t n,
                 off_t offset) override {
        if (offset < 0)
            return -EINVAL;

        n = std::min(n, buf_size);

        auto pFile = (procfs_file*)file->fs_data;

        if (!pFile->read)
            return -EACCES;
        if (pFile->children)
            return -EISDIR;

        using namespace kernel::mem;
        using namespace kernel::mem::paging;
        auto* page = alloc_page();
        auto pPageBuffer = physaddr<u8>(page_to_pfn(page));

        ssize_t nread = pFile->read(pPageBuffer, 4096);
        if (nread < offset) {
            free_page(page);

            return nread < 0 ? nread : 0;
        }

        n = std::min(n, (usize)nread - offset);
        memcpy(buf, pPageBuffer + offset, n);

        free_page(page);
        return n;
    }

    ssize_t readdir(inode* dir, size_t offset,
                    const filldir_func& callback) override {
        auto* pFile = (procfs_file*)dir->fs_data;
        if (!pFile->children)
            return -ENOTDIR;

        const auto& children = *pFile->children;

        usize cur = offset;
        for (; cur < pFile->children->size(); ++cur) {
            auto& file = children[cur];
            auto* inode = get_inode(file.ino);
            if (!inode) {
                inode = alloc_inode(file.ino);

                inode->fs_data = (void*)&file;
                inode->mode = _get_mode(file);
            }

            int ret = callback(file.name.c_str(), inode, 0);
            if (ret != 0)
                break;
        }

        return cur - offset;
    }
};

const procfs_file* root() { return s_root; }

const procfs_file* create(const procfs_file* parent, std::string name,
                          read_fn read, write_fn write) {
    auto& file = parent->children->emplace_back();

    file.name = std::move(name);
    file.ino = s_next_ino++;
    file.read = std::move(read);
    file.write = std::move(write);
    file.children = nullptr;

    return 0;
}

const procfs_file* mkdir(const procfs_file* parent, std::string name) {
    auto& file = parent->children->emplace_back();

    file.name = std::move(name);
    file.ino = s_next_ino++;
    file.read = nullptr;
    file.write = nullptr;
    file.children = new std::vector<procfs_file>();

    return &file;
}

class procfs_module : public virtual kernel::kmod::kmod {
   public:
    procfs_module() : kmod("procfs") {}

    virtual int init() override {
        int ret = procfs::init();
        if (ret < 0)
            return ret;

        return fs::register_fs("procfs", procfs::create);
    }
};

} // namespace kernel::procfs

INTERNAL_MODULE(procfs, procfs_module);
