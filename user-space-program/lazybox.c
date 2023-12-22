#include <unistd.h>
#include <dirent.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

struct applet {
    const char* name;
    int (*func)(const char** args);
};

int lazybox_version(void)
{
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

int ls(const char** args)
{
    const char* path = args[0];
    DIR* dir = NULL;

    if (path == NULL) {
        char buf[256];
        if (getcwd(buf, sizeof(buf)) == 0)
            return -1;

        dir = opendir(buf);
    } else {
        dir = opendir(args[0]);
    }

    if (!dir)
        return -1;

    struct dirent* dp = NULL;
    while ((dp = readdir(dir)) != NULL) {
        printf("%s ", dp->d_name);
    }

    printf("\n");

    return 0;
}

struct applet applets[] = {
    {
        "lazybox",
        NULL,
    },
    {
        "pwd",
        pwd,
    },
    {
        "ls",
        ls,
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
        if (*last == '/') {
            ++last;
            break;
        }
    }
    return last == path ? path : last + 1;
}

int parse_applet(const char* name)
{
    if (!name)
        return -1;

    for (size_t i = 0; i < (sizeof(applets) / sizeof(struct applet)); ++i) {
        if (strcmpi(applets[i].name, name) == 0) {
            return i;
        }
    }

    return -1;
}

int main(int argc, const char** argv)
{
    if (argc == 0)
        return lazybox_version();

    const char* name = find_file_name(*argv);
    int type = parse_applet(find_file_name(*argv));

    if (type < 0) {
        printf("applet not found: %s\n", name);
        return -1;
    }

    if (type == 0)
        return main(argc - 1, argv + 1);
    
    return applets[type].func(argv + 1);
}
