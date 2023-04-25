#ifndef __GBLIBC_SIGNAL_H_
#define __GBLIBC_SIGNAL_H_

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SIGINT 2
#define SIGQUIT 3
#define SIGPIPE 13
#define SIGSTOP 19

int kill(pid_t pid, int sig);
int raise(int sig);

#ifdef __cplusplus
}
#endif

#endif
