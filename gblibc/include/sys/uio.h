#ifndef __GBLIBC_SYS_UIO_H
#define __GBLIBC_SYS_UIO_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct iovec {
    void* iov_base;
    size_t iov_len;
};

#ifdef __cplusplus
}
#endif

#endif
