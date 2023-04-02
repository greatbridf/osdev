#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/wait.h>

#define print(str) write(STDERR_FILENO, str, strlen(str))

int main(int argc, char** argv)
{
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
        char* envp[1] = { NULL };

        if (argc < 2)
            shell_argv[0] = "/bin/sh";
        else
            shell_argv[0] = argv[1];
        
        for (int i = 2; i < argc; ++i)
            shell_argv[i - 1] = argv[i];
        
        execve(shell_argv[0], shell_argv, envp);

        print("[init] unable to run sh, exiting...\n");
        return -1;
    }

    int ret, pid;
    char buf[512] = {};
    for (;;) {
        pid = wait(&ret);
        snprintf(buf, sizeof(buf), "[init] pid%d has exited with code %d\n", pid, ret);
        print(buf);
        // sh
        if (pid == sh_pid)
            goto _run_sh;
    }

    return 0;
}
