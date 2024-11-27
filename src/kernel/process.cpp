#include <types/types.h>

// TODO: remove this
void NORETURN freeze(void) {
    for (;;)
        asm volatile("cli\n\thlt");
}
