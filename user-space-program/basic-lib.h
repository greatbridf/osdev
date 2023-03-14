#include <stdint.h>
#include <sys/types.h>

typedef uint32_t ino_t;

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

struct __attribute__((__packed__)) user_dirent {
    ino_t d_ino; // inode number
    uint32_t d_off; // ignored
    uint16_t d_reclen; // length of this struct user_dirent
    char d_name[1]; // file name with a padding zero
    // uint8_t d_type; // file type, with offset of (d_reclen - 1)
};

// static inline size_t getdents(int fd, struct user_dirent* buf, size_t cnt)
// {
//     return syscall(0x08, fd, (uint32_t)buf, cnt);
// }
