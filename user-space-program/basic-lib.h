typedef __UINT32_TYPE__ uint32_t;
typedef __UINT16_TYPE__ uint16_t;
typedef __UINT8_TYPE__ uint8_t;

static inline uint32_t syscall(uint32_t num, uint32_t arg1, uint32_t arg2)
{
    asm ("movl %3, %%eax\n\
          movl %1, %%edi\n\
          movl %2, %%esi\n\
          int $0x80\n\
          movl %%eax, %0"
         :"=r"(num)
         :"0"(arg1), "r"(arg2), "r"(num)
         :"eax", "edi", "esi"
         );
    return num;
}
