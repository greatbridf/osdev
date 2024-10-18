#ifndef __GBLIBC_SYS_UTSNAME_H
#define __GBLIBC_SYS_UTSNAME_H

#include <sys/types.h>

#ifdef __cplusplus
extern "C" {
#endif

#define OLD_UTSNAME_LENGTH 8

struct oldold_utsname {
    char sysname[9];
    char nodename[9];
    char release[9];
    char version[9];
    char machine[9];
};

#define UTSNAME_LENGTH 64

struct old_utsname {
    char sysname[65];
    char nodename[65];
    char release[65];
    char version[65];
    char machine[65];
};

struct new_utsname {
    char sysname[65];
    char nodename[65];
    char release[65];
    char version[65];
    char machine[65];
    char domainname[65];
};

#ifdef __cplusplus
}
#endif

#endif
