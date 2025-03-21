#ifndef __GBLIBC_UNISTD_H_
#define __GBLIBC_UNISTD_H_

#include <sys/types.h>

#undef STDOUT_FILENO
#undef STDIN_FILENO
#undef STDERR_FILENO
#define STDIN_FILENO (0)
#define STDOUT_FILENO (1)
#define STDERR_FILENO (2)

#define F_OK 0
#define R_OK 1
#define W_OK 2
#define X_OK 4

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

pid_t getpid(void);
pid_t getppid(void);

int setpgid(pid_t pid, pid_t pgid);

pid_t setsid(void);
pid_t getsid(pid_t pid);

pid_t tcgetpgrp(int fd);
int tcsetpgrp(int fd, pid_t pgrp);

int brk(void* addr);
void* sbrk(ssize_t increment);

int isatty(int fd);

extern char** environ;

#ifdef __cplusplus
}
#endif

#endif
