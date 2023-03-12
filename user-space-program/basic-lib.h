typedef __UINT32_TYPE__ uint32_t;
typedef __UINT16_TYPE__ uint16_t;
typedef __UINT8_TYPE__ uint8_t;

typedef uint32_t pid_t;
typedef uint32_t size_t;
typedef size_t ino_t;

#define GNU_ATTRIBUTE(attr) __attribute__((attr))
#define NORETURN GNU_ATTRIBUTE(noreturn)

#define O_RDONLY (0)
#define O_DIRECTORY (0x4)

// dirent file types
#define DT_UNKNOWN 0
#define DT_FIFO 1
#define DT_CHR 2
#define DT_DIR 4
#define DT_BLK 6
#define DT_REG 8
#define DT_LNK 10
#define DT_SOCK 12
#define DT_WHT 14

#define DT_MAX (S_DT_MASK + 1) /* 16 */

static inline uint32_t syscall(uint32_t num, uint32_t arg1, uint32_t arg2, uint32_t arg3)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%edx\n"
        "movl %4, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(num)
        : "g"(arg1), "g"(arg2), "g"(arg3), "g"(num)
        : "eax", "edx", "edi", "esi");
    return num;
}

static inline void NORETURN syscall_noreturn(uint32_t num, uint32_t arg1, uint32_t arg2, uint32_t arg3)
{
    asm volatile(
        "movl %1, %%edi\n"
        "movl %2, %%esi\n"
        "movl %3, %%edx\n"
        "movl %4, %%eax\n"
        "int $0x80\n"
        "movl %%eax, %0"
        : "=g"(num)
        : "g"(arg1), "g"(arg2), "g"(arg3), "g"(num)
        : "eax", "edx", "edi", "esi");
    // crash
    syscall_noreturn(0x05, 0, 0, 0);
}

static inline int fork(void)
{
    return syscall(0x00, 0, 0, 0);
}
static inline uint32_t write(int fd, const char* buf, size_t count)
{
    return syscall(0x01, fd, (uint32_t)buf, count);
}
static inline uint32_t read(int fd, char* buf, size_t count)
{
    return syscall(0x07, fd, (uint32_t)buf, count);
}
static inline void sleep(void)
{
    syscall(0x02, 0, 0, 0);
}
static inline void NORETURN exec(const char* bin, const char** argv)
{
    syscall_noreturn(0x04, (uint32_t)bin, (uint32_t)argv, 0);
}
static inline void NORETURN exit(int exit_code)
{
    syscall_noreturn(0x05, exit_code, 0, 0);
}
static inline uint32_t wait(int* return_value)
{
    return syscall(0x06, (uint32_t)return_value, 0, 0);
}

struct __attribute__((__packed__)) user_dirent {
    ino_t d_ino; // inode number
    uint32_t d_off; // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1]; // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

static inline size_t getdents(int fd, struct user_dirent* buf, size_t cnt)
{
    return syscall(0x08, fd, (uint32_t)buf, cnt);
}

static inline int open(const char* path, uint32_t flags)
{
    return syscall(0x09, (uint32_t)path, flags, 0);
}
