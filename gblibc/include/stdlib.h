#ifndef __GBLIBC_STDLIB_H_
#define __GBLIBC_STDLIB_H_

#ifdef __cplusplus
extern "C" {
#endif

int atoi(const char* str);

void __attribute__((noreturn)) exit(int status);

#ifdef __cplusplus
}
#endif

#endif
