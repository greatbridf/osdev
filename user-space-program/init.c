#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

#define print(str) write(STDERR_FILENO, str, _strlen(str))

static size_t _strlen(const char* str) {
    size_t len = 0;
    while (str[len] != '\0') {
        len++;
    }
    return len;
}

static __attribute__((used)) size_t strlen(const char* s) {
    size_t len = 0;
    while (*s++)
        ++len;
    return len;
}

static __attribute__((used)) void* memcpy(void* dst, const void* src,
                                          size_t n) {
    uint8_t* d = (uint8_t*)dst;
    const uint8_t* s = (const uint8_t*)src;
    for (size_t i = 0; i < n; ++i)
        d[i] = s[i];
    return dst;
}

int main(int argc, char** argv) {
    int fd = 0;
    // Assumes three file descriptors open.
    while ((fd = open("/dev/console", 0)) >= 0) {
        if (fd >= 3) {
            close(fd);
            break;
        }
    }

    print("***** GBOS INIT SYSTEM *****\n");

_run_sh:;
    pid_t sh_pid = fork();
    if (sh_pid < 0) {
        print("[init] unable to fork(), exiting...\n");
        return -1;
    }

    // child
    if (sh_pid == 0) {
        pid_t sid = setsid();
        if (sid < 0) {
            print("[init] unable to setsid, exiting...\n");
            return -1;
        }

        char* shell_argv[128] = {};

        if (argc < 2)
            shell_argv[0] = "/bin/sh";
        else
            shell_argv[0] = argv[1];

        for (int i = 2; i < argc; ++i)
            shell_argv[i - 1] = argv[i];

        execve(shell_argv[0], shell_argv, environ);

        print("[init] unable to run sh, exiting...\n");
        return -1;
    }

    int ret, pid;
    for (;;) {
        pid = wait(&ret);
        char* buf = NULL;
        assert(asprintf(&buf, "[init] pid%d has exited with code %d\n", pid,
                        ret) >= 0);
        print(buf);
        free(buf);
        // sh
        if (pid == sh_pid)
            goto _run_sh;
    }

    return 0;
}
