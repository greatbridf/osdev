#include <algorithm>
#include <string>

#include <assert.h>
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>

#include <types/allocator.hpp>

#include <fs/fat.hpp>
#include <kernel/mem/paging.hpp>
#include <kernel/mem/phys.hpp>
#include <kernel/module.hpp>
#include <kernel/vfs.hpp>

#define VFAT_FILENAME_LOWERCASE (0x08)
#define VFAT_EXTENSION_LOWERCASE (0x10)

namespace fs::fat {

// buf MUST be larger than 512 bytes
void fat32::_raw_read_sector(void* buf, uint32_t sector_no) {
    auto nread = _read_sector_range(buf, SECTOR_SIZE, sector_no, 1);

    assert((size_t)nread == SECTOR_SIZE);
}

// buf MUST be larger than 4096 bytes
void fat32::_raw_read_cluster(void* buf, cluster_t no) {
    // data cluster start from cluster #2
    no -= 2;

    auto nread = _read_sector_range(
        buf, sectors_per_cluster * SECTOR_SIZE,
        data_region_offset + no * sectors_per_cluster, sectors_per_cluster);

    assert((size_t)nread == SECTOR_SIZE * sectors_per_cluster);
}

ssize_t fat32::_read_sector_range(void* _buf, size_t buf_size,
                                  uint32_t sector_offset, size_t sector_cnt) {
    buf_size &= ~(SECTOR_SIZE - 1);

    sector_cnt = std::min(sector_cnt, buf_size / SECTOR_SIZE);

    auto* buf = (char*)_buf;

    auto n =
        block_device_read(m_device, buf, buf_size, sector_offset * SECTOR_SIZE,
                          sector_cnt * SECTOR_SIZE);

    return n;
}

char* fat32::read_cluster(cluster_t no) {
    auto iter = buf.find(no);
    if (iter) {
        auto& [idx, buf] = *iter;
        ++buf.ref;
        return buf.data;
    }
    // TODO: page buffer class
    using namespace kernel::mem;
    using namespace paging;
    assert(sectors_per_cluster * SECTOR_SIZE <= 0x1000);

    char* data = physaddr<char>{page_to_pfn(alloc_page())};
    _raw_read_cluster(data, no);
    buf.emplace(no, buf_object{data, 1});

    return data;
}

void fat32::release_cluster(cluster_t no) {
    auto iter = buf.find(no);
    if (iter)
        --iter->second.ref;
}

ssize_t fat32::readdir(inode* dir, size_t offset,
                       const vfs::filldir_func& filldir) {
    cluster_t next = cl(dir);
    for (size_t i = 0; i < (offset / (sectors_per_cluster * SECTOR_SIZE));
         ++i) {
        if (next >= EOC)
            return 0;
        next = fat[next];
    }
    size_t nread = 0;
    do {
        char* buf = read_cluster(next);
        auto* d = reinterpret_cast<directory_entry*>(buf) +
                  (offset % (sectors_per_cluster * SECTOR_SIZE)) /
                      sizeof(directory_entry);
        offset = 0;
        auto* end =
            d + (sectors_per_cluster * SECTOR_SIZE / sizeof(directory_entry));
        for (; d < end && d->filename[0]; ++d) {
            if (d->attributes.volume_label) {
                nread += sizeof(directory_entry);
                continue;
            }

            ino_t ino = _rearrange(d);
            auto* ind = get_inode(ino);
            if (!ind) {
                mode_t mode = 0777;
                if (d->attributes.subdir)
                    mode |= S_IFDIR;
                else
                    mode |= S_IFREG;

                ind = alloc_inode(ino);
                ind->size = d->size;
                ind->mode = mode;
                ind->nlink = d->attributes.subdir ? 2 : 1;
            }

            std::string fname;
            for (int i = 0; i < 8; ++i) {
                if (d->filename[i] == ' ')
                    break;
                if (d->_reserved & VFAT_FILENAME_LOWERCASE)
                    fname += tolower(d->filename[i]);
                else
                    fname += toupper(d->filename[i]);
            }
            if (d->extension[0] != ' ')
                fname += '.';
            for (int i = 1; i < 3; ++i) {
                if (d->extension[i] == ' ')
                    break;
                if (d->_reserved & VFAT_EXTENSION_LOWERCASE)
                    fname += tolower(d->extension[i]);
                else
                    fname += toupper(d->extension[i]);
            }
            auto ret = filldir(fname.c_str(), ind, ind->mode & S_IFMT);

            if (ret != 0) {
                release_cluster(next);
                return nread;
            }

            nread += sizeof(directory_entry);
        }
        release_cluster(next);
        next = fat[next];
    } while (next < EOC);
    return nread;
}

fat32::fat32(dev_t _device) : vfs(_device, 4096), label{} {
    auto* buf = new char[SECTOR_SIZE];
    _raw_read_sector(buf, 0);

    auto* info = reinterpret_cast<ext_boot_sector*>(buf);

    sector_cnt = info->sectors_cnt;
    sectors_per_fat = info->sectors_per_fat;
    sectors_per_cluster = info->old.sectors_per_cluster;
    serial_number = info->serial_number;
    root_dir = info->root_directory;
    reserved_sectors = info->old.reserved_sectors;
    fat_copies = info->old.fat_copies;

    data_region_offset = reserved_sectors + fat_copies * sectors_per_fat;

    // read file allocation table
    fat.resize(SECTOR_SIZE * sectors_per_fat / sizeof(cluster_t));
    _read_sector_range(fat.data(), SECTOR_SIZE * sectors_per_fat,
                       reserved_sectors, sectors_per_fat);

    int i = 0;
    while (i < 11 && info->label[i] != 0x20) {
        label[i] = info->label[i];
        ++i;
    }
    label[i] = 0x00;

    _raw_read_sector(buf, info->fs_info_sector);

    auto* fsinfo = reinterpret_cast<fs_info_sector*>(buf);
    free_clusters = fsinfo->free_clusters;
    next_free_cluster_hint = fsinfo->next_free_cluster;

    delete[] buf;

    size_t _root_dir_clusters = 1;
    cluster_t next = root_dir;
    while ((next = fat[next]) < EOC)
        ++_root_dir_clusters;

    auto* n = alloc_inode(root_dir);
    n->size = _root_dir_clusters * sectors_per_cluster * SECTOR_SIZE;
    n->mode = S_IFDIR | 0777;
    n->nlink = 2;

    register_root_node(n);
}

ssize_t fat32::read(inode* file, char* buf, size_t buf_size, size_t n,
                    off_t offset) {
    uint32_t cluster_size = SECTOR_SIZE * sectors_per_cluster;
    size_t orig_n = n;

    for (cluster_t cno = cl(file); n && cno < EOC; cno = fat[cno]) {
        if (offset >= cluster_size) {
            offset -= cluster_size;
            continue;
        }

        auto* data = read_cluster(cno);
        data += offset;

        auto to_copy = std::min(n, (size_t)(cluster_size - offset));
        auto ncopied = _write_buf_n(buf, buf_size, data, to_copy);

        buf += ncopied, n -= ncopied;

        release_cluster(cno);
        if (ncopied != to_copy)
            break;

        offset = 0;
    }

    return orig_n - n;
}

static fat32* create_fat32(const char* source, unsigned long, const void*) {
    // TODO: flags
    // TODO: parse source
    (void)source;
    return new fat32(fs::make_device(8, 1));
}

class fat32_module : public virtual kernel::module::module {
   public:
    fat32_module() : module("fat32") {}
    ~fat32_module() {
        // TODO: unregister filesystem
    }

    virtual int init() override {
        int ret = fs::register_fs("fat32", create_fat32);

        if (ret != 0)
            return kernel::module::MODULE_FAILED;

        return kernel::module::MODULE_SUCCESS;
    }
};

} // namespace fs::fat

static kernel::module::module* fat32_module_init() {
    return new fs::fat::fat32_module;
}

INTERNAL_MODULE(fat32_module_loader, fat32_module_init);
