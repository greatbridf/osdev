#include <cstddef>
#include <map>
#include <sys/types.h>
#include <vector>
#include <bit>
#include <utility>

#include <bits/alltypes.h>
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>

#include <kernel/log.hpp>
#include <kernel/mem.h>
#include <kernel/process.hpp>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>
#include <types/allocator.hpp>
#include <types/status.h>
#include <types/path.hpp>
#include <types/string.hpp>

struct tmpfs_file_entry {
    size_t ino;
    char filename[128];
};

fs::vfs::dentry::dentry(dentry* _parent, inode* _ind, name_type _name)
    : parent(_parent) , ind(_ind) , flags { } , name(_name)
{
    if (!_ind || S_ISDIR(ind->mode)) {
        children = new std::list<dentry>;
        idx_children = new types::hash_map<name_type, dentry*>;
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
    if (!S_ISDIR(ind->mode))
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
        errno = ENOENT;
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
    const dentry& root, types::path &out_dst) const
{
    const dentry* dents[32];
    int cnt = 0;

    const dentry* cur = this;
    while (cur != &root) {
        assert(cnt < 32);
        dents[cnt++] = cur;
        cur = cur->parent;
    }

    out_dst.append("/");
    for (int i = cnt - 1; i >= 0; --i)
        out_dst.append(dents[i]->name.c_str());
}

fs::inode* fs::vfs::cache_inode(size_t size, ino_t ino,
    mode_t mode, uid_t uid, gid_t gid)
{
    auto [ iter, inserted ] =
        _inodes.try_emplace(ino, inode { ino, this, size, mode, uid, gid });
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

    if (!S_ISDIR(ind->mode)) {
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
    if (!S_ISDIR(mnt->ind->mode)) {
        errno = ENOTDIR;
        return GB_FAILED;
    }

    auto* new_ent = new_fs->root();

    new_ent->parent = mnt->parent;
    new_ent->name = mnt->name;

    auto* orig_ent = mnt->replace(new_ent);
    _mount_recover_list.emplace(new_ent, orig_ent);

    return GB_OK;
}
size_t fs::vfs::inode_read(inode*, char*, size_t, size_t, size_t)
{ return -EINVAL; }
size_t fs::vfs::inode_write(inode*, const char*, size_t, size_t)
{ return -EINVAL; }
int fs::vfs::inode_mkfile(dentry*, const char*, mode_t)
{ return -EINVAL; }
int fs::vfs::inode_mknode(dentry*, const char*, mode_t, dev_t)
{ return -EINVAL; }
int fs::vfs::inode_rmfile(dentry*, const char*)
{ return -EINVAL; }
int fs::vfs::inode_mkdir(dentry*, const char*, mode_t)
{ return -EINVAL; }
int fs::vfs::inode_statx(dentry*, statx*, unsigned int)
{ return -EINVAL; }
int fs::vfs::inode_stat(dentry*, struct stat*)
{ return -EINVAL; }
dev_t fs::vfs::inode_devid(fs::inode*)
{ return -EINVAL; }

class tmpfs : public virtual fs::vfs {
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
            auto ret = filldir(entry.filename, 0, entry.ino, ind->mode & S_IFMT);
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

    virtual int inode_mkfile(dentry* dir, const char* filename, mode_t mode) override
    {
        auto& file = *cache_inode(0, _savedata(mk_data_vector()), S_IFREG | mode, 0, 0);
        mklink(dir->ind, &file, filename);
        dir->append(get_inode(file.ino), filename, true);
        return GB_OK;
    }

    virtual int inode_mknode(dentry* dir, const char* filename, mode_t mode, dev_t dev) override
    {
        if (!S_ISBLK(mode) && !S_ISCHR(mode))
            return -EINVAL;

        auto& node = *cache_inode(0, _savedata(dev), mode, 0, 0);
        mklink(dir->ind, &node, filename);
        dir->append(get_inode(node.ino), filename, true);
        return GB_OK;
    }

    virtual int inode_mkdir(dentry* dir, const char* dirname, mode_t mode) override
    {
        auto new_dir = cache_inode(0, _savedata(mk_fe_vector()), S_IFDIR | (mode & 0777), 0, 0);
        mklink(new_dir, new_dir, ".");

        mklink(dir->ind, new_dir, dirname);
        mklink(new_dir, dir->ind, "..");

        dir->append(new_dir, dirname, true);
        return GB_OK;
    }

    virtual size_t inode_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n) override
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

    virtual size_t inode_write(fs::inode* file, const char* buf, size_t offset, size_t n) override
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

    virtual int inode_statx(dentry* dent, statx* st, unsigned int mask) override
    {
        auto* ind = dent->ind;
        const mode_t mode = ind->mode;

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

    virtual dev_t inode_devid(fs::inode* file) override
    {
        return as_val(_getdata(file->ino));
    }
};

fs::regular_file::regular_file(vfs::dentry* parent,
    file_flags flags, size_t cursor, inode* ind)
    : file(ind->mode, parent, flags), cursor(cursor), ind(ind) { }

ssize_t fs::regular_file::read(char* __user buf, size_t n)
{
    if (!flags.read)
        return -EBADF;

    if (S_ISDIR(ind->mode))
        return -EISDIR;

    // TODO: copy to user function !IMPORTANT
    ssize_t n_wrote = fs::vfs_read(ind, buf, n, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

ssize_t fs::regular_file::write(const char* __user buf, size_t n)
{
    if (!flags.write)
        return -EBADF;

    if (S_ISDIR(ind->mode))
        return -EISDIR;

    // TODO: check privilege of user ptr
    ssize_t n_wrote = fs::vfs_write(ind, buf, cursor, n);
    if (n_wrote >= 0)
        cursor += n_wrote;

    return n_wrote;
}

int fs::regular_file::getdents(char* __user buf, size_t cnt)
{
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->inode_readdir(ind, cursor,
        [&buf, &cnt](const char* fn, size_t len, ino_t ino, uint8_t type) {
            if (!len)
                len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent) + 1 + len;
            if (cnt < reclen)
                return GB_FAILED;

            auto* dirp = (fs::user_dirent*)buf;
            dirp->d_ino = ino;
            dirp->d_reclen = reclen;
            // TODO: show offset
            // dirp->d_off = 0;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, len);
            buf[reclen - 2] = 0;
            buf[reclen - 1] = type;

            buf += reclen;
            cnt -= reclen;
            return GB_OK;
        });

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

int fs::regular_file::getdents64(char* __user buf, size_t cnt)
{
    if (!S_ISDIR(ind->mode))
        return -ENOTDIR;

    size_t orig_cnt = cnt;
    int nread = ind->fs->inode_readdir(ind, cursor,
        [&buf, &cnt](const char* fn, size_t len, ino_t ino, uint8_t type) {
            if (!len)
                len = strlen(fn);

            size_t reclen = sizeof(fs::user_dirent64) + len;
            if (cnt < reclen)
                return GB_FAILED;

            auto* dirp = (fs::user_dirent64*)buf;
            dirp->d_ino = ino;
            dirp->d_off = 114514;
            dirp->d_reclen = reclen;
            dirp->d_type = type;
            // TODO: use copy_to_user
            memcpy(dirp->d_name, fn, len);
            buf[reclen - 1] = 0;

            buf += reclen;
            cnt -= reclen;
            return GB_OK;
        });

    if (nread > 0)
        cursor += nread;

    return orig_cnt - cnt;
}

fs::fifo_file::fifo_file(vfs::dentry* parent, file_flags flags,
    std::shared_ptr<fs::pipe> ppipe)
    : file(S_IFIFO, parent, flags), ppipe(ppipe) { }

ssize_t fs::fifo_file::read(char* __user buf, size_t n)
{
    if (!flags.read)
        return -EBADF;

    return ppipe->read(buf, n);
}

ssize_t fs::fifo_file::write(const char* __user buf, size_t n)
{
    if (!flags.write)
        return -EBADF;

    return ppipe->write(buf, n);
}

fs::fifo_file::~fifo_file()
{
    assert(flags.read ^ flags.write);
    if (flags.read)
        ppipe->close_read();
    else
        ppipe->close_write();
}

static std::map<dev_t, fs::blkdev_ops> blkdevs;
static std::map<dev_t, fs::chrdev_ops> chrdevs;

size_t fs::vfs_read(fs::inode* file, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->inode_read(file, buf, buf_size, offset, n);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev = file->fs->inode_devid(file);

        ssize_t ret;
        if (S_ISBLK(file->mode))
            ret = block_device_read(dev, buf, buf_size, offset, n);
        else
            ret = char_device_read(dev, buf, buf_size, n);

        if (ret < 0) {
            errno = -ret;
            return -1U;
        }

        return ret;
    }

    errno = EINVAL;
    return -1U;
}
size_t fs::vfs_write(fs::inode* file, const char* buf, size_t offset, size_t n)
{
    if (S_ISDIR(file->mode)) {
        errno = EISDIR;
        return -1U;
    }

    if (S_ISREG(file->mode))
        return file->fs->inode_write(file, buf, offset, n);

    if (S_ISBLK(file->mode) || S_ISCHR(file->mode)) {
        dev_t dev = file->fs->inode_devid(file);

        ssize_t ret;
        if (S_ISBLK(file->mode))
            ret = block_device_write(dev, buf, offset, n);
        else
            ret = char_device_write(dev, buf, n);

        if (ret < 0) {
            errno = -ret;
            return -1U;
        }

        return ret;
    }

    errno = EINVAL;
    return -1U;
}
int fs::vfs_mkfile(fs::vfs::dentry* dir, const char* filename, mode_t mode)
{
    return dir->ind->fs->inode_mkfile(dir, filename, mode);
}
int fs::vfs_mknode(fs::vfs::dentry* dir, const char* filename, mode_t mode, dev_t dev)
{
    return dir->ind->fs->inode_mknode(dir, filename, mode, dev);
}
int fs::vfs_rmfile(fs::vfs::dentry* dir, const char* filename)
{
    return dir->ind->fs->inode_rmfile(dir, filename);
}
int fs::vfs_mkdir(fs::vfs::dentry* dir, const char* dirname, mode_t mode)
{
    return dir->ind->fs->inode_mkdir(dir, dirname, mode);
}

fs::vfs::dentry* fs::vfs_open(fs::vfs::dentry& root, const types::path& path)
{
    fs::vfs::dentry* cur = &root;

    for (const auto& item : path) {
        if (item.empty())
            continue;
        cur = cur->find(item);
        if (!cur)
            return nullptr;
    }

    return cur;
}

int fs::vfs_stat(fs::vfs::dentry* ent, statx* stat, unsigned int mask)
{
    return ent->ind->fs->inode_statx(ent, stat, mask);
}

static std::list<fs::vfs*, types::memory::ident_allocator<fs::vfs*>> fs_es;

int fs::register_block_device(dev_t node, fs::blkdev_ops ops)
{
    auto iter = blkdevs.find(node);
    if (iter)
        return -EEXIST;

    std::tie(iter, std::ignore) = blkdevs.emplace(node, std::move(ops));
    return 0;
}

int fs::register_char_device(dev_t node, fs::chrdev_ops ops)
{
    auto iter = chrdevs.find(node);
    if (iter)
        return -EEXIST;

    std::tie(iter, std::ignore) = chrdevs.emplace(node, std::move(ops));
    return 0;
}

// MBR partition table, used by partprobe()

struct PACKED mbr_part_entry {
    uint8_t attr;
    uint8_t chs_start[3];
    uint8_t type;
    uint8_t chs_end[3];
    uint32_t lba_start;
    uint32_t cnt;
};

struct PACKED mbr {
    uint8_t code[440];
    uint32_t signature;
    uint16_t reserved;
    mbr_part_entry parts[4];
    uint16_t magic;
};

static inline void mbr_part_probe(dev_t node, char ch)
{
    mbr buf_mbr;
    // TODO: devtmpfs
    auto* dev = fs::vfs_open(*fs::fs_root, "/dev");
    if (!dev)
        return;

    char label[] = "sda1";
    label[2] = ch;
    auto ret = fs::block_device_read(node, (char*)&buf_mbr, sizeof(mbr), 0, 512);
    if (ret < 0) {
        kmsg("[kernel] cannot read device for part probing.\n");
        return;
    }

    int n = 1;
    for (const auto& part : buf_mbr.parts) {
        if (n >= 8)
            break;

        if (!part.type)
            continue;

        std::size_t part_offset = part.lba_start * 512;

        // TODO: add partition offset limit
        fs::register_block_device(node + n, {
            [=](char* buf, size_t buf_size, size_t offset, size_t n) -> ssize_t {
                offset += part_offset;
                return fs::block_device_read(node, buf, buf_size, offset, n);
            },
            [=](const char* buf, size_t offset, size_t n) -> ssize_t {
                offset += part_offset;
                return fs::block_device_write(node, buf, offset, n);
            }
        });

        ret = fs::vfs_mknode(dev, label, 0660 | S_IFBLK, node + n);
        ++n, ++label[3];
    }
}

void fs::partprobe()
{
    auto* dev = fs::vfs_open(*fs::fs_root, "/dev");
    if (!dev)
        return;

    char ch = 'a';
    char name[] = "sd*";
    types::string<> path = "/dev/sd*";
    for (const auto& device : blkdevs) {
        // only the devices whose minor number is a multiple of 8
        // are considered as a disk instead of partitions
        if (NODE_MINOR(device.first) % 8 != 0)
            continue;

        path.pop();
        path += ch;
        name[2] = ch;

        auto* blkfile = fs::vfs_open(*fs::fs_root, path.c_str());
        if (!blkfile)
            vfs_mknode(dev, name, 0660 | S_IFBLK, device.first);

        mbr_part_probe(device.first, ch);

        ++ch;
    }
}

ssize_t fs::block_device_read(dev_t node, char* buf, size_t buf_size, size_t offset, size_t n)
{
    if (node == fs::NODE_INVALID)
        return -EINVAL;

    auto iter = blkdevs.find(node);
    if (!iter || !iter->second.read)
        return -EINVAL;

    return iter->second.read(buf, buf_size, offset, n);
}

ssize_t fs::block_device_write(dev_t node, const char* buf, size_t offset, size_t n)
{
    if (node == fs::NODE_INVALID)
        return -EINVAL;

    auto iter = blkdevs.find(node);
    if (!iter || !iter->second.write)
        return -EINVAL;

    return iter->second.write(buf, offset, n);
}

ssize_t fs::char_device_read(dev_t node, char* buf, size_t buf_size, size_t n)
{
    if (node == fs::NODE_INVALID)
        return -EINVAL;

    auto iter = chrdevs.find(node);
    if (!iter || !iter->second.read)
        return -EINVAL;

    return iter->second.read(buf, buf_size, n);
}

ssize_t fs::char_device_write(dev_t node, const char* buf, size_t n)
{
    if (node == fs::NODE_INVALID)
        return -EINVAL;

    auto iter = chrdevs.find(node);
    if (!iter || !iter->second.read)
        return -EINVAL;

    return iter->second.write(buf, n);
}

fs::vfs* fs::register_fs(vfs* fs)
{
    fs_es.push_back(fs);
    return fs;
}

ssize_t b_null_read(char* buf, size_t buf_size, size_t n)
{
    if (n >= buf_size)
        n = buf_size;
    memset(buf, 0x00, n);
    return n;
}
ssize_t b_null_write(const char*, size_t n)
{
    return n;
}
static ssize_t console_read(char* buf, size_t buf_size, size_t n)
{
    return console->read(buf, buf_size, n);
}
static ssize_t console_write(const char* buf, size_t n)
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
            current_thread->send_signal(SIGPIPE);
            return -EPIPE;
        }

        while (this->buf.avail() < n) {
            if (!m_cv.wait(mtx))
                return -EINTR;

            if (!is_readable()) {
                current_thread->send_signal(SIGPIPE);
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
    register_char_device(make_device(1, 0), { b_null_read, b_null_write });
    // console (supports serial console only for now)
    // TODO: add interface to bind console device to other devices
    register_char_device(make_device(2, 0), { console_read, console_write });

    auto* rootfs = new tmpfs;
    fs_es.push_back(rootfs);
    fs_root = rootfs->root();

    vfs_mkdir(fs_root, "dev", 0755);
    vfs_mkdir(fs_root, "mnt", 0755);
    vfs_mkfile(fs_root, "init", 0755);

    auto* init = vfs_open(*fs_root, "/init");
    assert(init);
    const char* str = "#!/mnt/busybox sh\n"
                      "cd /\n"
                      "busybox mkdir etc\n"
                      "busybox mkdir root\n"
                      "busybox cat > /etc/passwd <<EOF\n"
                      "root:x:0:0:root:/root:/mnt/busybox_ sh\n"
                      "EOF\n"
                      "busybox cat > /etc/group <<EOF\n"
                      "root:x:0:root\n"
                      "EOF\n"
                      "exec /mnt/init /mnt/busybox_ sh < /dev/console"
                      "    >> /dev/console 2>>/dev/console\n";
    vfs_write(init->ind, str, 0, strlen(str));

    auto* dev = vfs_open(*fs_root, "/dev");
    assert(dev);
    vfs_mknode(dev, "null", 0666 | S_IFCHR, make_device(1, 0));
    vfs_mknode(dev, "console", 0666 | S_IFCHR, make_device(2, 0));
}
