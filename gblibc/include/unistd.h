#ifndef __GBLIBC_UNISTD_H_
#define __GBLIBC_UNISTD_H_

#include <sys/types.h>

#undef STDOUT_FILENO
#undef STDIN_FILENO
#undef STDERR_FILENO
#define STDIN_FILENO (0)
#define STDOUT_FILENO (1)
#define STDERR_FILENO (2)

#ifdef __cplusplus
extern "C" {
#endif

ssize_t read(int fd, void* buf, size_t count);
ssize_t write(int fd, const void* buf, size_t count);

int dup(int oldfd);
int dup2(int oldfd, int newfd);

int pipe(int pipefd[2]);

int close(int fd);

void __attribute__((noreturn)) _exit(int code);
pid_t fork(void);
int execve(const char* pathname, char* const argv[], char* const envp[]);

unsigned int sleep(unsigned int seconds);

int chdir(const char* path);
char* getcwd(char* buf, size_t bufsize);

int setpgid(pid_t pid, pid_t pgid);

pid_t setsid(void);
pid_t getsid(pid_t pid);

#ifdef __cplusplus
}
#endif

#endif
