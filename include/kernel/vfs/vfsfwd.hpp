#pragma once

namespace fs {

// in dentry.hpp
struct dentry;

// in file.hpp
struct file;
struct regular_file;
struct fifo_file;

class pipe;

// in filearray.hpp
class file_array;

// in inode.hpp
struct inode;

// in vfs.hpp
class vfs;

} // namespace fs
