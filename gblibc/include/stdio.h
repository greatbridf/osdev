#ifndef __GBLIBC_STDIO_H_
#define __GBLIBC_STDIO_H_

#include <stdarg.h>
#include <stdint.h>

#undef EOF
#define EOF (-1)

#undef BUFSIZ
#define BUFSIZ (1024)

#ifdef __cplusplus
extern "C" {
#endif

typedef struct __io_file {
    int fd;
    uint32_t flags;

    char* rbuf;
    size_t rpos;
    size_t rcnt;
    size_t rbsz;

    char* wbuf;
    size_t wpos;
    size_t wbsz;
} FILE;

int putchar(int character);
int getchar(void);

int puts(const char* str);
char* gets(char* str);

int vsnprintf(char* buf, size_t bufsize, const char* fmt, va_list args);
int snprintf(char* buf, size_t bufsize, const char* fmt, ...);
int sprintf(char* buf, const char* fmt, ...);

int vfprintf(FILE* stream, const char* fmt, va_list args);
int fprintf(FILE* stream, const char* fmt, ...);

int vprintf(const char* fmt, va_list args);
int printf(const char* fmt, ...);

FILE* fopen(const char* path, const char* mode);
int fflush(FILE* stream);
int fclose(FILE* stream);

int getc_unlocked(FILE* stream);
int putc_unlocked(int character, FILE* stream);
int fputs_unlocked(const char* s, FILE* stream);
int fputc_unlocked(int character, FILE* stream);
int fputs(const char* s, FILE* stream);
int fgetc(FILE* stream);
int fputc(int character, FILE* stream);

int ferror(FILE* stream);
int feof(FILE* stream);
void clearerr(FILE* stream);

extern FILE* stdout;
extern FILE* stdin;
extern FILE* stderr;
#undef stdout
#undef stdin
#undef stderr
#define stdout (stdout)
#define stdin (stdin)
#define stderr (stderr)

#ifdef __cplusplus
}
#endif

#endif
