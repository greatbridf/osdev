#ifndef __GBLIBC_UNISTD_H_
#define __GBLIBC_UNISTD_H_

#include <sys/types.h>

#undef STDOUT_FILENO
#undef STDIN_FILENO
#undef STDERR_FILENO
#define STDOUT_FILENO (0)
#define STDIN_FILENO (0)
#define STDERR_FILENO (0)

#ifdef __cplusplus
extern "C" {
#endif

ssize_t read(int fd, void* buf, size_t count);
ssize_t write(int fd, const void* buf, size_t count);

void __attribute__((noreturn)) _exit(int code);
pid_t fork(void);
int execve(const char* pathname, char* const argv[], char* const envp[]);

unsigned int sleep(unsigned int seconds);

int chdir(const char* path);
char* getcwd(char* buf, size_t bufsize);

#ifdef __cplusplus
}
#endif

#endif
