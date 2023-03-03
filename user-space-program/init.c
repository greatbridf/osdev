#include "basic-lib.h"

int main(int argc, char** argv)
{
    for (int i = 0; i < argc; ++i)
        write(0, argv[i], 0);

    const char* data = "Hello World from user space init\n";
    write(0, data, 33);
    int ret = fork();
    if (ret == 0) {
        write(0, "child\n", 6);
        exit(255);
    } else {
        write(0, "parent\n", 7);
    }

    char buf[128] = {};
    for (;;) {
        int n = read(0, buf, 5);
        if (n)
            write(0, buf, n);
        else
            write(0, "fuck!\n", 6);
        
        if (buf[0] == 'e' && buf[1] == 'x' && buf[2] == 'i' && buf[3] == 't') {
            write(0, "\nexited echo mode!\n", 19);
            break;
        }
    }

    for (;;) {
        int ret;
        pid_t pid = wait(&ret);
        (void)pid;
        pid += 1000;
    }
    return 0;
}
