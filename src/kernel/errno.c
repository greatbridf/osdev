#include <kernel/errno.h>
#include <types/types.h>

uint32_t* _get_errno(void)
{
    static uint32_t _errno = 0;
    return &_errno;
}
