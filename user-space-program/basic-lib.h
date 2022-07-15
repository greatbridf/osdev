typedef __UINT32_TYPE__ uint32_t;
typedef __UINT16_TYPE__ uint16_t;
typedef __UINT8_TYPE__ uint8_t;

static inline uint32_t syscall(uint32_t num, uint32_t arg1, uint32_t arg2)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(num)
        : "g"(arg1), "g"(arg2), "g"(num)
        : "eax", "edx", "edi", "esi");
    return num;
}
