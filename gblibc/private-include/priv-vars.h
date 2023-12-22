#ifndef __GBLIBC_PRIV_VARS_H_
#define __GBLIBC_PRIV_VARS_H_

#include <list.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct mem {
    uint32_t sz;
    uint32_t flag;
};

#define FILE_READ (1 << 0)
#define FILE_WRITE (1 << 1)
#define FILE_ERROR (1 << 2)
#define FILE_EOF (1 << 3)

void** __start_brk_location(void);
void** __curr_brk_location(void);
list_head* __io_files_location(void);
size_t* __environ_size_location(void);

#undef start_brk
#define start_brk (*__start_brk_location())
#undef curr_brk
#define curr_brk (*__curr_brk_location())

#undef iofiles
#define iofiles (*__io_files_location())

#undef environ_size
#define environ_size (*__environ_size_location())

#ifdef __cplusplus
}
#endif

#endif
