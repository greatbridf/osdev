#include "basic-lib.h"

static inline char tc(int n) {
    return '0' + n;
}

void print_int(int n) {
    char buf[12];
    int len = 0;

    while (n) {
        int dig = n % 10;
        buf[len++] = tc(dig);
        n /= 10;
    }

    write(0, buf, len);
}

size_t strlen(const char* str)
{
    const char* p = str;
    while (*p)
        ++p;
    return p - str;
}

static inline void print(const char* str)
{
    write(0, str, strlen(str));
}

int main(int argc, char** argv)
{
    for (int i = 0; i < argc; ++i)
        write(0, argv[i], 0);

    print("Hello World from user space init\n");
    int ret = fork();
    if (ret == 0) {
        print("child\n");
        exit(255);
    } else {
        print("parent\n");
    }

    char buf[128] = {};

    const char* path = "/dev";
    print("content in ");
    print(path);
    print(":\n");

    int dir = open(path, O_RDONLY | O_DIRECTORY);
    if (dir >= 0) {
        for (;;) {
            int n = getdents(dir, (struct user_dirent*)buf, 128);
            if (n < 0)
                print("error\n");
            if (n <= 0)
                break;

            int bpos = 0;
            for (; bpos < n;) {
                struct user_dirent* dirp = (struct user_dirent*)(buf + bpos);
                print("ino: ");
                print_int(dirp->d_ino);
                print(", filename: \"");
                print(dirp->d_name);
                print("\", filetype: ");
                switch (buf[bpos + dirp->d_reclen - 1]) {
                    case DT_REG:
                        print("regular file");
                        break;
                    case DT_DIR:
                        print("directory");
                        break;
                    case DT_BLK:
                        print("block device");
                        break;
                    default:
                        print("unknown");
                        break;
                }
                print("\n");
                bpos += dirp->d_reclen;
            }
        }
    }

    for (;;) {
        int n = read(0, buf, 128);
        if (n)
            write(0, buf, n);
        else
            print("fuck!\n");
        
        if (buf[0] == 'e' && buf[1] == 'x' && buf[2] == 'i' && buf[3] == 't') {
            print("\nexited echo mode!\n");
            break;
        }
    }

    for (;;) {
        int ret;
        pid_t pid = wait(&ret);
        print("ret: ");
        print_int(ret);
        print(", pid: ");
        print_int(pid);
        print("\n");
    }
    return 0;
}
