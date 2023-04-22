#ifndef __GBLIBC_PRIV_VARS_H_
#define __GBLIBC_PRIV_VARS_H_

#include <list.h>

#ifdef __cplusplus
extern "C" {
#endif

#define FILE_READ (1 << 0)
#define FILE_WRITE (1 << 1)

void** __start_brk_location(void);
void** __curr_brk_location(void);
list_head* __io_files_location(void);

#undef start_brk
#define start_brk (*__start_brk_location())
#undef curr_brk
#define curr_brk (*__curr_brk_location())

#undef iofiles
#define iofiles (*__io_files_location())

#ifdef __cplusplus
}
#endif

#endif
