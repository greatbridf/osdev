#include <types/assert.h>

void operator delete(void*)
{
    crash();
}
void operator delete(void*, unsigned int)
{
    crash();
}
