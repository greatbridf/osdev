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

void __init_gblibc(void)
{
    // initialize program break position
    start_brk = curr_brk = (void*)syscall1(SYS_brk, (uint32_t)NULL);

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

    NDINSERT(iofiles, node);

    // stdin
    node = NEWNODE(FILE);
    stdin = &NDDATA(*node, FILE);
    memset(stdin, 0x00, sizeof(FILE));

    stdin->fd = STDIN_FILENO;
    stdin->flags = FILE_READ;
    stdin->rbuf = malloc(BUFSIZ);
    stdin->rbsz = BUFSIZ;

    NDINSERT(iofiles, node);

    // stderr
    node = NEWNODE(FILE);
    stderr = &NDDATA(*node, FILE);
    memset(stderr, 0x00, sizeof(FILE));

    stderr->fd = STDERR_FILENO;
    stderr->flags = FILE_WRITE;

    NDINSERT(iofiles, node);
}
