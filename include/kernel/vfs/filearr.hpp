#pragma once

#include <memory>

#include "dentry.hpp"
#include "file.hpp"

namespace fs {

class filearray {
private:
    struct impl;
    std::shared_ptr<impl> pimpl;
    filearray(std::shared_ptr<impl>);

public:
    filearray();
    filearray(filearray&& other) = default;

    filearray copy() const;
    filearray share() const;

    // dup old_fd to some random fd
    int dup(int old_fd);

    // dup old_fd to new_fd, close new_fd if it is already open
    int dup(int old_fd, int new_fd, int flags);

    // dup old_fd to the first available fd starting from min_fd
    int dupfd(int fd, int min_fd, int flags);

    fs::file* operator[](int i) const;
    int set_flags(int fd, int flags);

    int pipe(int (&pipefd)[2]);
    int open(dentry& root, const types::path& filepath, int flags, mode_t mode);

    int close(int fd);

    // any call to member methods will be invalid after clear()
    void clear();
    void onexec();
};

} // namespace fs
