#ifndef __GBLIBC_UNISTD_H_
#define __GBLIBC_UNISTD_H_

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

ssize_t read(int fd, void* buf, size_t count);
ssize_t write(int fd, const void* buf, size_t count);

_Noreturn void _exit(int code);
pid_t fork(void);
int execve(const char* pathname, char* const argv[], char* const envp[]);

unsigned int sleep(unsigned int seconds);

#ifdef __cplusplus
}
#endif

#endif
