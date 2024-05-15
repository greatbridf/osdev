#ifndef __GBLIBC_BITS_IOCTL_H_
#define __GBLIBC_BITS_IOCTL_H_

#include <sys/uio.h>

#define TCGETS (0x5401)
#define TCSETS (0x5402)
#define TIOCGPGRP (0x540f)
#define TIOCSPGRP (0x5410)
#define TIOCGWINSZ (0x5413)

#ifdef __cplusplus
extern "C" {
#endif

struct winsize {
    unsigned short ws_row;
    unsigned short ws_col;
    unsigned short ws_xpixel;
    unsigned short ws_ypixel;
};

#ifdef __cplusplus
}
#endif

#endif
