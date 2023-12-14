#include <map>
#include <vector>
#include <bit>
#include <utility>

#include <assert.h>
#include <kernel/errno.h>
#include <kernel/mem.h>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>
#include <stdint.h>
#include <stdio.h>
#include <types/allocator.hpp>
#include <types/status.h>
#include <types/string.hpp>

using types::allocator_traits;
using types::kernel_allocator;
using types::string;

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

fs::vfs::dentry::dentry(dentry* _parent, inode* _ind, name_type _name)
    : parent(_parent) , ind(_ind) , flags { } , name(_name)
{
    if (!_ind || _ind->flags.in.directory) {
        children = types::pnew<allocator_type>(children);
        idx_children = types::pnew<allocator_type>(idx_children);
    }
}

fs::vfs::dentry* fs::vfs::dentry::append(inode* ind, const name_type& name, bool set_dirty)
{
    auto& ent = children->emplace_back(this, ind, name);
    idx_children->emplace(ent.name, &ent);
    if (set_dirty)
        this->flags.in.dirty = 1;
    return &ent;
}
fs::vfs::dentry* fs::vfs::dentry::append(inode* ind, name_type&& name, bool set_dirty)
{
    auto& ent = children->emplace_back(this, ind, std::move(name));
    idx_children->emplace(ent.name, &ent);
    if (set_dirty)
        this->flags.in.dirty = 1;
    return &ent;
}
fs::vfs::dentry* fs::vfs::dentry::find(const name_type& name)
{
    if (!ind->flags.in.directory)
        return nullptr;

    if (name[0] == '.') {
        if (!name[1])
            return this;
        if (name[1] == '.' && !name[2])
            return parent ? parent : this;
    }

    if (!flags.in.present)
        ind->fs->load_dentry(this);

    auto iter = idx_children->find(name);
    if (!iter) {
        errno = ENOTFOUND;
        return nullptr;
    }

    return iter->second;
}
fs::vfs::dentry* fs::vfs::dentry::replace(dentry* val)
{
    // TODO: prevent the dirent to be swapped out of memory
    parent->idx_children->find(this->name)->second = val;
    return this;
}
void fs::vfs::dentry::invalidate(void)
{
    // TODO: write back
    flags.in.dirty = 0;
    children->clear();
    idx_children->clear();
    flags.in.present = 0;
}
fs::vfs::vfs(void)
    : _root(nullptr, nullptr, "")
{
}

void fs::vfs::dentry::path(
    const dentry& root, name_type &out_dst) const
{
    const dentry* dents[32];
    int cnt = 0;

    const dentry* cur = this;
    while (cur != &root) {
        assert(cnt < 32);
        dents[cnt++] = cur;
        cur = cur->parent;
    }

    if (cnt == 0) {
        out_dst += '/';
        return;
    }

    for (int i = cnt - 1; i >= 0; --i) {
        out_dst += '/';
        out_dst += dents[i]->name;
    }
}

fs::inode* fs::vfs::cache_inode(inode_flags flags, uint32_t perm, size_t size, ino_t ino)
{
    auto [ iter, inserted ] =
        _inodes.try_emplace(ino, inode { flags, perm, ino, this, size });
    return &iter->second;
}
fs::inode* fs::vfs::get_inode(ino_t ino)
{
    auto iter = _inodes.find(ino);
    // TODO: load inode from disk if not found
    if (iter)
        return &iter->second;
    else
        return nullptr;
}
void fs::vfs::register_root_node(inode* root)
{
    if (!_root.ind)
        _root.ind = root;
}
int fs::vfs::load_dentry(dentry* ent)
{
    auto* ind = ent->ind;

    if (!ind->flags.in.directory) {
        errno = ENOTDIR;
        return GB_FAILED;
    }

    size_t offset = 0;

    for (int ret = 1; ret > 0; offset += ret) {
        ret = this->inode_readdir(ind, offset,
            [ent, this](const char* name, size_t len, ino_t ino, uint8_t) -> int {
                if (!len)
                    ent->append(get_inode(ino), name, false);
                else
                    ent->append(get_inode(ino), dentry::name_type(name, len), false);

                return GB_OK;
            });
    }

    ent->flags.in.present = 1;

    return GB_OK;
}
int fs::vfs::mount(dentry* mnt, vfs* new_fs)
{
    if (!mnt->ind->flags.in.directory) {
        errno = ENOTDIR;
        return GB_FAILED;
    }

    auto* new_ent = new_fs->root();

    new_ent->parent = mnt->parent;
    new_ent->name = mnt->name;

    auto* orig_ent = mnt->replace(new_ent);
    _mount_recover_list.emplace(new_ent, orig_ent);

    new_ent->ind->flags.in.mount_point = 1;

    return GB_OK;
}
size_t fs::vfs::inode_read(inode*, char*, size_t, size_t, size_t)
{
    assert(false);
    return 0xffffffff;
}
size_t fs::vfs::inode_write(inode*, const char*, size_t, size_t)
{
    assert(false);
    return 0xffffffff;
}
int fs::vfs::inode_mkfile(dentry*, const char*)
{
    assert(false);
    return GB_FAILED;
}
int fs::vfs::inode_mknode(dentry*, const char*, node_t)
{
    assert(false);
    return GB_FAILED;
}
int fs::vfs::inode_rmfile(dentry*, const char*)
{
    assert(false);
    return GB_FAILED;
}
int fs::vfs::inode_mkdir(dentry*, const char*)
{
    assert(false);
    return GB_FAILED;
}
int fs::vfs::inode_stat(dentry*, stat*)
{
    assert(false);
    return GB_FAILED;
}
uint32_t fs::vfs::inode_getnode(fs::inode*)
{
    assert(false);
    return 0xffffffff;
}

class tmpfs : public virtual fs::vfs {
private:
    using fe_t = tmpfs_file_entry;
    using vfe_t = std::vector<fe_t>;
    using fdata_t = std::vector<char>;

private:
    std::map<fs::ino_t, void*> inode_data;
    fs::ino_t _next_ino;

private:
    fs::ino_t _assign_ino(void)
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
    inline void* _getdata(fs::ino_t ino) const
    {
        return inode_data.find(ino)->second;
    }
    inline fs::ino_t _savedata(void* data)
    {
        fs::ino_t ino = _assign_ino();
        inode_data.insert(std::make_pair(ino, data));
        return ino;
    }
    inline fs::ino_t _savedata(ptr_t data)
    {
        return _savedata((void*)data);
    }

protected:
    inline vfe_t* mk_fe_vector(void)
    {
        return allocator_traits<kernel_allocator<vfe_t>>::allocate_and_construct();
    }

    inline fdata_t* mk_data_vector(void)
    {
        return allocator_traits<kernel_allocator<fdata_t>>::allocate_and_construct();
    }

    void mklink(fs::inode* dir, fs::inode* inode, const char* filename)
    {
        auto* fes = as_vfe(_getdata(dir->ino));
        fes->emplace_back(fe_t {
            .ino = inode->ino,
            .filename = {} });
        auto& emplaced = fes->back();
        strncpy(emplaced.filename, filename, sizeof(emplaced.filename));
        emplaced.filename[sizeof(emplaced.filename) - 1] = 0;
        dir->size += sizeof(fe_t);
    }

    virtual int inode_readdir(fs::inode* dir, size_t offset, const fs::vfs::filldir_func& filldir) override
    {
        if (!dir->flags.in.directory) {
            return -1;
        }

        auto& entries = *as_vfe(_getdata(dir->ino));
        size_t off = offset / sizeof(fe_t);

        size_t nread = 0;

        for (; (off + 1) <= entries.size(); ++off, nread += sizeof(fe_t)) {
            const auto& entry = entries[off];
            auto* ind = get_inode(entry.ino);

            auto type = DT_REG;
            if (ind->flags.in.directory)
                type = DT_DIR;
            if (ind->flags.in.special_node)
                type = DT_BLK;

            auto ret = filldir(entry.filename, 0, entry.ino, type);
            if (ret != GB_OK)
                break;
        }

        return nread;
    }

public:
    explicit tmpfs(void)
        : _next_ino(1)
    {
        auto& in = *cache_inode({ INODE_DIR | INODE_MNT }, 0777, 0, _savedata(mk_fe_vector()));

        mklink(&in, &in, ".");
        mklink(&in, &in, "..");

        register_root_node(&in);
    }

    virtual int inode_mkfile(dentry* dir, const char* filename) override
    {
        auto& file = *cache_inode({ .v = INODE_FILE }, 0777, 0, _savedata(mk_data_vector()));
        mklink(dir->ind, &file, filename);
        dir->append(get_inode(file.ino), filename, true);
        return GB_OK;
    }

    virtual int inode_mknode(dentry* dir, const char* filename, fs::node_t sn) override
    {
        auto& node = *cache_inode({ .v = INODE_NODE }, 0777, 0, _savedata(sn.v));
        mklink(dir->ind, &node, filename);
        dir->append(get_inode(node.ino), filename, true);
        return GB_OK;
    }

    virtual int inode_mkdir(dentry* dir, const char* dirname) override
    {
        auto new_dir = cache_inode({ .v = INODE_DIR }, 0777, 0, _savedata(mk_fe_vector()));
        mklink(new_dir, new_dir, ".");

        mklink(dir->ind, new_dir, dirname);
        mklink(new_dir, dir->ind, "..");

        dir->append(new_dir, dirname, true);
        return GB_OK;
    }

    virtual size_t inode_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override
    {
        if (file->flags.in.file != 1)
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

    virtual size_t inode_write(fs::inode* file, const char* buf, size_t offset, size_t n) override
    {
        if (file->flags.in.file != 1)
            return 0;

        auto* data = as_fdata(_getdata(file->ino));

        for (size_t i = data->size(); i < offset + n; ++i) {
            data->push_back(0);
        }
        memcpy(data->data() + offset, buf, n);

        return n;
    }

    virtual int inode_stat(dentry* dir, fs::stat* stat) override
    {
        auto* file_inode = dir->ind;

        stat->st_ino = file_inode->ino;
        stat->st_size = file_inode->size;
        if (file_inode->flags.in.file) {
            stat->st_rdev.v = 0;
            stat->st_blksize = 1;
            stat->st_blocks = file_inode->size;
        }
        if (file_inode->flags.in.directory) {
            stat->st_rdev.v = 0;
            stat->st_blksize = sizeof(fe_t);
            stat->st_blocks = file_inode->size;
        }
        if (file_inode->flags.in.special_node) {
            stat->st_rdev.v = as_val(_getdata(file_inode->ino));
            stat->st_blksize = 0;
            stat->st_blocks = 0;
        }

        return GB_OK;
    }

    virtual uint32_t inode_getnode(fs::inode* file) override
    {
        return as_val(_getdata(file->ino));
    }
};

// 8 * 8 for now
static fs::special_node sns[8][8];

size_t fs::vfs_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        uint32_t ret = file->fs->inode_getnode(file);
        if (ret == SN_INVALID) {
            errno = EINVAL;
            return 0xffffffff;
        }
        fs::node_t sn {
            .v = ret
        };
        auto* ptr = &sns[sn.in.major][sn.in.minor];
        auto* ops = &ptr->ops;
        if (ops && ops->read)
            return ops->read(ptr, buf, buf_size, offset, n);
        else {
            errno = EINVAL;
            return 0xffffffff;
        }
    } else {
        return file->fs->inode_read(file, buf, buf_size, offset, n);
    }
}
size_t fs::vfs_write(fs::inode* file, const char* buf, size_t offset, size_t n)
{
    if (file->flags.in.special_node) {
        uint32_t ret = file->fs->inode_getnode(file);
        if (ret == SN_INVALID) {
            errno = EINVAL;
            return 0xffffffff;
        }
        fs::node_t sn {
            .v = ret
        };
        auto* ptr = &sns[sn.in.major][sn.in.minor];
        auto* ops = &ptr->ops;
        if (ops && ops->write)
            return ops->write(ptr, buf, offset, n);
        else {
            errno = EINVAL;
            return 0xffffffff;
        }
    } else {
        return file->fs->inode_write(file, buf, offset, n);
    }
}
int fs::vfs_mkfile(fs::vfs::dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_mkfile(dir, filename);
}
int fs::vfs_mknode(fs::vfs::dentry* dir, const char* filename, fs::node_t sn)
{
    return dir->ind->fs->inode_mknode(dir, filename, sn);
}
int fs::vfs_rmfile(fs::vfs::dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_rmfile(dir, filename);
}
int fs::vfs_mkdir(fs::vfs::dentry* dir, const char* dirname)
{
    return dir->ind->fs->inode_mkdir(dir, dirname);
}

fs::vfs::dentry* fs::vfs_open(
    fs::vfs::dentry& root,
    const char* pwd, const char* path)
{
    fs::vfs::dentry* cur = nullptr;
    if (!*path)
        return nullptr;

    // absolute path
    if (*path == '/' || !pwd)
        cur = &root;
    else
        cur = vfs_open(root, nullptr, pwd);

    size_t n = 0;
    string curpath;
    while (true) {
        switch (path[n]) {
        case '\0':
            if (n != 0) {
                curpath.clear();
                curpath.append(path, n);
                cur = cur->find(curpath);
            }
            return cur;

        case '/':
            if (n == 0) {
                ++path;
                continue;
            }

            curpath.clear();
            curpath.append(path, n);
            cur = cur->find(curpath);

            if (!cur)
                return cur;

            path += (n + 1);
            n = 0;
            break;

        default:
            ++n;
            break;
        }
    }
}

int fs::vfs_stat(fs::vfs::dentry* ent, stat* stat)
{
    return ent->ind->fs->inode_stat(ent, stat);
}

static std::list<fs::vfs*>* fs_es;

void fs::register_special_block(
    uint16_t major,
    uint16_t minor,
    fs::special_node_read read,
    fs::special_node_write write,
    uint32_t data1,
    uint32_t data2)
{
    fs::special_node& sn = sns[major][minor];
    sn.ops.read = read;
    sn.ops.write = write;
    sn.data1 = data1;
    sn.data2 = data2;
}

fs::vfs* fs::register_fs(vfs* fs)
{
    fs_es->push_back(fs);
    return fs;
}

size_t b_null_read(fs::special_node*, char* buf, size_t buf_size, size_t, size_t n)
{
    if (n >= buf_size)
        n = buf_size;
    memset(buf, 0x00, n);
    return n;
}
size_t b_null_write(fs::special_node*, const char*, size_t, size_t n)
{
    return n;
}
static size_t console_read(fs::special_node*, char* buf, size_t buf_size, size_t, size_t n)
{
    return console->read(buf, buf_size, n);
}
static size_t console_write(fs::special_node*, const char* buf, size_t, size_t n)
{
    size_t orig_n = n;
    while (n--)
        console->putchar(*(buf++));

    return orig_n;
}

fs::pipe::pipe(void)
    : buf { PIPE_SIZE }
    , flags { READABLE | WRITABLE }
{
}

void fs::pipe::close_read(void)
{
    {
        types::lock_guard lck(m_cv.mtx());
        flags &= (~READABLE);
    }
    m_cv.notify_all();
}

void fs::pipe::close_write(void)
{
    {
        types::lock_guard lck(m_cv.mtx());
        flags &= (~WRITABLE);
    }
    m_cv.notify_all();
}

int fs::pipe::write(const char* buf, size_t n)
{
    // TODO: check privilege
    // TODO: check EPIPE
    {
        auto& mtx = m_cv.mtx();
        types::lock_guard lck(mtx);

        if (!is_readable()) {
            current_process->signals.set(kernel::SIGPIPE);
            return -EPIPE;
        }

        while (this->buf.avail() < n) {
            if (!m_cv.wait(mtx))
                return -EINTR;

            if (!is_readable()) {
                current_process->signals.set(kernel::SIGPIPE);
                return -EPIPE;
            }
        }

        for (size_t i = 0; i < n; ++i)
            this->buf.put(*(buf++));
    }

    m_cv.notify();
    return n;
}

int fs::pipe::read(char* buf, size_t n)
{
    // TODO: check privilege
    {
        auto& mtx = m_cv.mtx();
        types::lock_guard lck(mtx);

        if (!is_writeable()) {
            size_t orig_n = n;
            while (!this->buf.empty() && n--)
                *(buf++) = this->buf.get();

            return orig_n - n;
        }

        while (this->buf.size() < n) {
            if (!m_cv.wait(mtx))
                return -EINTR;

            if (!is_writeable()) {
                size_t orig_n = n;
                while (!this->buf.empty() && n--)
                    *(buf++) = this->buf.get();

                return orig_n - n;
            }
        }

        for (size_t i = 0; i < n; ++i)
            *(buf++) = this->buf.get();
    }

    m_cv.notify();
    return n;
}

SECTION(".text.kinit")
void init_vfs(void)
{
    using namespace fs;
    // null
    register_special_block(0, 0, b_null_read, b_null_write, 0, 0);
    // console (supports serial console only for now)
    // TODO: add interface to bind console device to other devices
    register_special_block(1, 0, console_read, console_write, 0, 0);

    fs_es = types::pnew<types::kernel_ident_allocator>(fs_es);

    auto* rootfs = new tmpfs;
    fs_es->push_back(rootfs);
    fs_root = rootfs->root();

    vfs_mkdir(fs_root, "dev");
    vfs_mkdir(fs_root, "root");
    vfs_mkdir(fs_root, "mnt");
    vfs_mkfile(fs_root, "init");

    auto* init = vfs_open(*fs_root, nullptr, "/init");
    assert(init);
    const char* str = "#/bin/sh\nexec /bin/sh\n";
    vfs_write(init->ind, str, 0, strlen(str));

    auto* dev = vfs_open(*fs_root, nullptr, "/dev");
    assert(dev);
    vfs_mknode(dev, "null", { .in { .major = 0, .minor = 0 } });
    vfs_mknode(dev, "console", { .in { .major = 1, .minor = 0 } });
    vfs_mknode(dev, "hda", { .in { .major = 2, .minor = 0 } });
}
