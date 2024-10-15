#include <set>

#include <assert.h>

#include <types/path.hpp>

#include <kernel/async/lock.hpp>
#include <kernel/vfs.hpp>
#include <kernel/vfs/dentry.hpp>
#include <kernel/vfs/filearr.hpp>

using namespace fs;

using kernel::async::mutex, kernel::async::lock_guard;

struct fditem {
    int fd;
    int flags;
    std::shared_ptr<file> pfile;
};

struct fditem_comparator {
    constexpr bool operator()(const fditem& lhs, const fditem& rhs) const {
        return lhs.fd < rhs.fd;
    }

    constexpr bool operator()(int fd, const fditem& rhs) const {
        return fd < rhs.fd;
    }

    constexpr bool operator()(const fditem& lhs, int fd) const {
        return lhs.fd < fd;
    }
};

// ALL METHODS SHOULD BE CALLED WITH LOCK HELD
struct filearray::impl {
    mutex mtx;

    const fs_context* context;
    std::set<fditem, fditem_comparator> arr;
    int min_avail{};

    int allocate_fd(int from);
    void release_fd(int fd);
    int next_fd();

    int do_dup(const fditem& oldfile, int new_fd, int flags);
    int place_new_file(std::shared_ptr<file> pfile, int flags);
};

int filearray::impl::allocate_fd(int from) {
    if (from < min_avail)
        from = min_avail;

    if (from == min_avail) {
        int nextfd = min_avail + 1;
        auto iter = arr.find(nextfd);
        while (iter && nextfd == iter->fd)
            ++nextfd, ++iter;

        int retval = min_avail;
        min_avail = nextfd;
        return retval;
    }

    int fd = from;
    auto iter = arr.find(fd);
    while (iter && fd == iter->fd)
        ++fd, ++iter;

    return fd;
}

void filearray::impl::release_fd(int fd) {
    if (fd < min_avail)
        min_avail = fd;
}

int filearray::impl::next_fd() {
    return allocate_fd(min_avail);
}

int filearray::impl::do_dup(const fditem& oldfile, int new_fd, int flags) {
    bool inserted;
    std::tie(std::ignore, inserted) = arr.emplace(new_fd, flags, oldfile.pfile);
    assert(inserted);

    return new_fd;
}

int filearray::impl::place_new_file(std::shared_ptr<file> pfile, int flags) {
    int fd = next_fd();

    bool inserted;
    std::tie(std::ignore, inserted) = arr.emplace(fd, std::move(flags), pfile);
    assert(inserted);

    return fd;
}

int filearray::dup(int old_fd) {
    lock_guard lck{pimpl->mtx};

    auto iter = pimpl->arr.find(old_fd);
    if (!iter)
        return -EBADF;

    int fd = pimpl->next_fd();
    return pimpl->do_dup(*iter, fd, 0);
}

int filearray::dup(int old_fd, int new_fd, int flags) {
    lock_guard lck{pimpl->mtx};

    auto iter_old = pimpl->arr.find(old_fd);
    if (!iter_old)
        return -EBADF;

    auto iter_new = pimpl->arr.find(new_fd);
    if (iter_new) {
        iter_new->pfile = iter_old->pfile;
        iter_new->flags = flags;

        return new_fd;
    }

    int fd = pimpl->allocate_fd(new_fd);
    assert(fd == new_fd);
    return pimpl->do_dup(*iter_old, fd, flags);
}

int filearray::dupfd(int fd, int min_fd, int flags) {
    lock_guard lck{pimpl->mtx};

    auto iter = pimpl->arr.find(fd);
    if (!iter)
        return -EBADF;

    int new_fd = pimpl->allocate_fd(min_fd);
    return pimpl->do_dup(*iter, new_fd, flags);
}

int filearray::set_flags(int fd, int flags) {
    lock_guard lck{pimpl->mtx};

    auto iter = pimpl->arr.find(fd);
    if (!iter)
        return -EBADF;

    iter->flags |= flags;
    return 0;
}

int filearray::close(int fd) {
    lock_guard lck{pimpl->mtx};

    auto iter = pimpl->arr.find(fd);
    if (!iter)
        return -EBADF;

    pimpl->release_fd(fd);
    pimpl->arr.erase(iter);

    return 0;
}

static inline std::pair<dentry_pointer, int> _open_file(
    const fs_context& context, const dentry_pointer& cwd,
    types::string_view filepath, int flags, mode_t mode) {
    auto [dent, ret] = fs::open(context, cwd, filepath);
    if (!dent)
        return {nullptr, ret};

    if (!(r_dentry_is_invalid(dent.get()))) {
        if ((flags & O_CREAT) && (flags & O_EXCL))
            return {nullptr, -EEXIST};
        return {std::move(dent), 0};
    }

    if (!(flags & O_CREAT))
        return {nullptr, -ENOENT};

    // create file
    if (int ret = fs_creat(dent.get(), mode); ret != 0)
        return {nullptr, ret};

    return {std::move(dent), 0};
}

// TODO: file opening permissions check
int filearray::open(const dentry_pointer& cwd, types::string_view filepath,
                    int flags, mode_t mode) {
    lock_guard lck{pimpl->mtx};

    auto [dent, ret] = _open_file(*pimpl->context, cwd, filepath, flags, mode);

    assert(dent || ret != 0);
    if (ret != 0)
        return ret;

    auto inode = r_dentry_get_inode(dent.get());
    auto filemode = r_get_inode_mode(inode);

    int fdflag = (flags & O_CLOEXEC) ? FD_CLOEXEC : 0;

    file::file_flags fflags;
    fflags.read = !(flags & O_WRONLY);
    fflags.write = (flags & (O_WRONLY | O_RDWR));
    fflags.append = S_ISREG(filemode) && (flags & O_APPEND);

    // check whether dentry is a file if O_DIRECTORY is set
    if (flags & O_DIRECTORY) {
        if (!S_ISDIR(filemode))
            return -ENOTDIR;
    } else {
        if (S_ISDIR(filemode) && fflags.write)
            return -EISDIR;
    }

    // truncate file
    if (flags & O_TRUNC) {
        if (fflags.write && S_ISREG(filemode)) {
            auto ret = fs_truncate(inode, 0);
            if (ret != 0)
                return ret;
        }
    }

    return pimpl->place_new_file(
        std::make_shared<regular_file>(fflags, 0, inode), fdflag);
}

int filearray::pipe(int (&pipefd)[2]) {
    lock_guard lck{pimpl->mtx};

    if (1) {
        std::shared_ptr<fs::pipe> ppipe{new fs::pipe};

        pipefd[0] = pimpl->place_new_file(
            std::make_shared<fifo_file>(file::file_flags{1, 0, 0}, ppipe), 0);

        pipefd[1] = pimpl->place_new_file(
            std::make_shared<fifo_file>(file::file_flags{0, 1, 0}, ppipe), 0);
    }

    return 0;
}

filearray::filearray(std::shared_ptr<impl> ptr) : pimpl{ptr} {}

filearray::filearray(const fs_context* context)
    : filearray{std::make_shared<impl>()} {
    pimpl->context = context;
}

filearray filearray::copy() const {
    lock_guard lck{pimpl->mtx};
    filearray ret{pimpl->context};

    ret.pimpl->min_avail = pimpl->min_avail;
    ret.pimpl->arr = pimpl->arr;

    return ret;
}

filearray filearray::share() const {
    return filearray{pimpl};
}

void filearray::clear() {
    pimpl.reset();
}

void filearray::onexec() {
    lock_guard lck{pimpl->mtx};

    for (auto iter = pimpl->arr.begin(); iter;) {
        if (!(iter->flags & FD_CLOEXEC)) {
            ++iter;
            continue;
        }
        pimpl->release_fd(iter->fd);
        iter = pimpl->arr.erase(iter);
    }
}

file* filearray::operator[](int i) const {
    lock_guard lck{pimpl->mtx};

    auto iter = pimpl->arr.find(i);
    if (!iter)
        return nullptr;

    return iter->pfile.get();
}
