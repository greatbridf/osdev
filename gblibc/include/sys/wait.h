#ifndef __GBLIBC_SYS_WAIT_H
#define __GBLIBC_SYS_WAIT_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

pid_t wait(int* code);
pid_t waitpid(pid_t pid, int* code, int options);

#ifdef __cplusplus
}
#endif

#endif
