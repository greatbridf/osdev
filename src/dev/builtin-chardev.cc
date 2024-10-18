#include <kernel/module.hpp>
#include <kernel/tty.hpp>
#include <kernel/vfs.hpp>

using namespace kernel::kmod;
using namespace kernel::tty;

static ssize_t null_read(char*, size_t, size_t) {
    return 0;
}

static ssize_t null_write(const char*, size_t n) {
    return n;
}

static ssize_t zero_read(char* buf, size_t buf_size, size_t n) {
    if (n > buf_size)
        n = buf_size;

    memset(buf, 0, n);
    return n;
}

static ssize_t zero_write(const char*, size_t n) {
    return n;
}

// TODO: add interface to bind console device to other devices
ssize_t console_read(char* buf, size_t buf_size, size_t n) {
    return console->read(buf, buf_size, n);
}

ssize_t console_write(const char* buf, size_t n) {
    size_t orig_n = n;
    while (n--)
        console->putchar(*(buf++));

    return orig_n;
}

class builtin_chardev : public virtual kmod {
   public:
    builtin_chardev() : kmod("builtin-chardev") {}
    int init() override {
        using namespace fs;
        // null
        chrdev_ops null_ops{
            .read = null_read,
            .write = null_write,
        };
        register_char_device(make_device(1, 3), null_ops);

        // zero
        chrdev_ops zero_ops{
            .read = zero_read,
            .write = zero_write,
        };
        register_char_device(make_device(1, 5), zero_ops);

        // console
        chrdev_ops console_ops{
            .read = console_read,
            .write = console_write,
        };
        register_char_device(make_device(5, 1), console_ops);

        return 0;
    }
};

INTERNAL_MODULE(builtin_chardev, builtin_chardev);
