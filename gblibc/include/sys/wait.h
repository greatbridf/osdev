#ifndef __GBLIBC_SYS_WAIT_H
#define __GBLIBC_SYS_WAIT_H

#include <sys/types.h>

#define WNOHANG 1
#define WUNTRACED 2

#define WEXISTATUS(s) (((s) & 0xff00) >> 8)
#define WTERMSIG(s) ((s) & 0x7f)
#define WSTOPSIG(s) WEXITSTATUS(s)
#define WCOREDUMP(s) ((s) & 0x80)
#define WIFEXITED(s) (!WTERMSIG(s))
#define WIFSTOPPED(s) (((s) & 0x7f) == 0x7f)
#define WIFSIGNALED(s) (WTERMSIG(s) && !WIFSTOPPED(s))
#define WIFCONTINUED(s) ((s) == 0xffff)

#ifdef __cplusplus
extern "C" {
#endif

pid_t wait(int* code);
pid_t waitpid(pid_t pid, int* code, int options);

#ifdef __cplusplus
}
#endif

#endif
