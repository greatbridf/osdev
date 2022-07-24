#include "basic-lib.h"

int main(int argc, char** argv)
{
    for (int i = 0; i < argc; ++i)
        write(argv[i]);

    const char* data = "Hello World from user space init\n";
    write(data);
    int ret = fork();
    if (ret == 0) {
        write("child\n");
        exit(255);
    } else {
        write("parent\n");
    }

    for (;;) {
        int code = wait();
        (void)code;
        code += 1000;
    }
    return 0;
}
