#pragma once

#include <defs.hpp>
#include <string>
#include <vector>

#include <sys/types.h>

namespace kernel::procfs {

using read_fn = std::function<isize(u8*, usize)>;
using write_fn = std::function<isize(const u8*, usize)>;

struct procfs_file {
    std::string name;
    ino_t ino;

    read_fn read;
    write_fn write;
    std::vector<procfs_file>* children;
};

const procfs_file* root();

const procfs_file* find(const procfs_file* parent, std::string name);
const procfs_file* mkdir(const procfs_file* parent, std::string name);
const procfs_file* create(const procfs_file* parent, std::string name,
                          read_fn read, write_fn write);

} // namespace kernel::procfs
