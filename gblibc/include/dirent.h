#ifndef __GBLIBC_DIRENT_H_
#define __GBLIBC_DIRENT_H_

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

struct dirent {
    ino_t d_ino;
    off_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[256];
};

typedef struct _DIR {
    int fd;
    struct dirent dent;
    char buffer[232];
    int bpos;
    int blen;
} DIR;

DIR* opendir(const char* name);
DIR* fdopendir(int fd);

struct dirent* readdir(DIR* dirp);

#ifdef __cplusplus
}
#endif

#endif
