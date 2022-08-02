typedef __UINT32_TYPE__ uint32_t;
typedef __UINT16_TYPE__ uint16_t;
typedef __UINT8_TYPE__ uint8_t;

typedef uint32_t pid_t;

#define GNU_ATTRIBUTE(attr) __attribute__((attr))
#define NORETURN GNU_ATTRIBUTE(noreturn)

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

static inline void NORETURN syscall_noreturn(uint32_t num, uint32_t arg1, uint32_t arg2)
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
    // crash
    syscall_noreturn(0x05, 0, 0);
}

static inline int fork(void)
{
    return syscall(0x00, 0, 0);
}
static inline uint32_t write(const char* buf)
{
    return syscall(0x01, (uint32_t)buf, 0);
}
static inline void sleep(void)
{
    syscall(0x02, 0, 0);
}
static inline void NORETURN exec(const char* bin, const char** argv)
{
    syscall_noreturn(0x04, (uint32_t)bin, (uint32_t)argv);
}
static inline void NORETURN exit(int exit_code)
{
    syscall_noreturn(0x05, exit_code, 0);
}
static inline uint32_t wait(int* return_value)
{
    return syscall(0x06, (uint32_t)return_value, 0);
}
