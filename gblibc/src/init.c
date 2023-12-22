#include <assert.h>
#include <priv-vars.h>
#include <stdlib.h>
#include <syscall.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <list.h>

FILE* stdout;
FILE* stdin;
FILE* stderr;

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
    memset(stdout, 0x00, sizeof(FILE));

    stdout->fd = STDOUT_FILENO;
    stdout->flags = FILE_WRITE;
    stdout->wbuf = malloc(BUFSIZ);
    stdout->wbsz = BUFSIZ;

    NDINSERT(&iofiles, node);

    // stdin
    node = NEWNODE(FILE);
    stdin = &NDDATA(*node, FILE);
    memset(stdin, 0x00, sizeof(FILE));

    stdin->fd = STDIN_FILENO;
    stdin->flags = FILE_READ;
    stdin->rbuf = malloc(BUFSIZ);
    stdin->rbsz = BUFSIZ;

    NDINSERT(&iofiles, node);

    // stderr
    node = NEWNODE(FILE);
    stderr = &NDDATA(*node, FILE);
    memset(stderr, 0x00, sizeof(FILE));

    stderr->fd = STDERR_FILENO;
    stderr->flags = FILE_WRITE;

    NDINSERT(&iofiles, node);
}
