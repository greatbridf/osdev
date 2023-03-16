#ifndef __GBLIBC_ASSERT_H_
#define __GBLIBC_ASSERT_H_

#ifdef NDEBUG
#define assert(st) ((void)(st))
#else
#define assert(st) ((void)((st) || (__assert_fail(#st, __FILE__, __LINE__, __func__), 0)))
#endif

#ifdef __cplusplus
extern "C" {
#endif

void __attribute__((noreturn))
__assert_fail(const char* statement, const char* file, int line, const char* func);

#ifdef __cplusplus
}
#endif

#endif
