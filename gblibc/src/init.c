#include <assert.h>
#include <priv-vars.h>
#include <stdlib.h>
#include <syscall.h>
#include <stdio.h>
#include <unistd.h>
#include <list.h>

FILE* stdout;
FILE* stdin;
FILE* stderr;

#define BYTES_PER_MAX_COPY_UNIT (sizeof(uint32_t) / sizeof(uint8_t))
static void* _memset(void* _dst, int c, size_t n)
{
    uint8_t* dst = (uint8_t*)_dst;
    c &= 0xff;
    int cc = (c + (c << 8) + (c << 16) + (c << 24));
    for (size_t i = 0; i < n / BYTES_PER_MAX_COPY_UNIT; ++i) {
        *(uint32_t*)dst = cc;
        dst += BYTES_PER_MAX_COPY_UNIT;
    }
    for (size_t i = 0; i < (n % BYTES_PER_MAX_COPY_UNIT); ++i) {
        *((char*)dst++) = c;
    }
    return dst;
}

static char* strchr(const char* s, int c)
{
    while (*s) {
        if (*s == c)
            return (char*)s;
        ++s;
    }
    return NULL;
}

list_head* __io_files_location(void)
{
    static list_head __io_files;
    return &__io_files;
}

size_t* __environ_size_location(void)
{
    static size_t __environ_size;
    return &__environ_size;
}

void __init_gblibc(int argc, char** argv, char** envp)
{
    (void)argc, (void)argv;
    // initialize program break position and heap
    start_brk = curr_brk = (void*)syscall1(SYS_brk, (uint32_t)NULL);

    sbrk(128 * 1024);
    struct mem* first = start_brk;
    first->sz = 0;
    first->flag = 0;

    // save environ vector
    environ_size = 4;
    environ = malloc(environ_size * sizeof(char*));
    assert(environ);

    while (*envp) {
        char* eqp = strchr(*envp, '=');
        if (!eqp || eqp == *envp)
            goto next;

        *eqp = 0;
        char* value = eqp + 1;
        setenv(*envp, value, 1);

    next:;
        ++envp;
    }

    // stdout, stdin, stderr objects
    list_node* node = NULL;

    // stdout
    node = NEWNODE(FILE);
    stdout = &NDDATA(*node, FILE);
    _memset(stdout, 0x00, sizeof(FILE));

    stdout->fd = STDOUT_FILENO;
    stdout->flags = FILE_WRITE;
    stdout->wbuf = malloc(BUFSIZ);
    stdout->wbsz = BUFSIZ;

    NDINSERT(&iofiles, node);

    // stdin
    node = NEWNODE(FILE);
    stdin = &NDDATA(*node, FILE);
    _memset(stdin, 0x00, sizeof(FILE));

    stdin->fd = STDIN_FILENO;
    stdin->flags = FILE_READ;
    stdin->rbuf = malloc(BUFSIZ);
    stdin->rbsz = BUFSIZ;

    NDINSERT(&iofiles, node);

    // stderr
    node = NEWNODE(FILE);
    stderr = &NDDATA(*node, FILE);
    _memset(stderr, 0x00, sizeof(FILE));

    stderr->fd = STDERR_FILENO;
    stderr->flags = FILE_WRITE;

    NDINSERT(&iofiles, node);
}
