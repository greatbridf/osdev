#include "unistd.h"
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

struct applet {
    const char* name;
    int (*func)(const char** args);
};

int puts(const char* str)
{
    size_t ret = write(STDOUT_FILENO, str, strlen(str));
    ret += write(STDOUT_FILENO, "\n", 1);
    return ret;
}

int printf(const char* fmt, ...)
{
    va_list args;
    va_start(args, fmt);

    char buf[128];
    int n = vsnprintf(buf, sizeof(buf), fmt, args);
    n = write(STDOUT_FILENO, buf, n);

    va_end(args);
    return n;
}

int lazybox_version(const char** _)
{
    (void)_;
    printf("lazybox by greatbridf\n");
    return 0;
}

int pwd(const char** _)
{
    (void)_;
    char buf[256];
    if (getcwd(buf, sizeof(buf)) == 0) {
        printf("cannot get cwd\n");
        return -1;
    }
    puts(buf);
    return 0;
}

struct applet applets[] = {
    {
        "lazybox",
        lazybox_version,
    },
    {
        "pwd",
        pwd,
    }
};

static inline int tolower(int c)
{
    if (c >= 'A' && c <= 'Z')
        return c - 'A' + 'a';
    return c;
}

int strcmpi(const char* a, const char* b)
{
    int ret = 0;
    while (*a && *b) {
        if (tolower(*a) != tolower(*b)) {
            ret = 1;
            break;
        }
        ++a, ++b;
    }
    if ((*a && !*b) || (*b && !*a)) {
        ret = 1;
    }
    return ret;
}

const char* find_file_name(const char* path)
{
    const char* last = path + strlen(path);
    for (; last != path; --last) {
        if (*last == '/')
            break;
    }
    return last + 1;
}

int parse_applet(const char* name)
{
    for (size_t i = 0; i < (sizeof(applets) / sizeof(struct applet)); ++i) {
        if (strcmpi(applets[i].name, name) == 0) {
            return i;
        }
    }

    return -1;
}

int main(int argc, const char** argv)
{
    int offset = 0;
    const char* name = find_file_name(argv[offset++]);
    int type = -1;

run:
    type = parse_applet(name);
    if (type == -1) {
        printf("applet not found: %s\n", name);
        return -1;
    }

    if (type == 0 && argc != 1) {
        name = argv[offset++];
        goto run;
    }

    return applets[type].func(argv + offset);
}
