#include <alloca.h>

void* alloca(size_t size)
{
    return __builtin_alloca(size);
}
