#ifndef __GBLIBC_PRIV_VARS_H_
#define __GBLIBC_PRIV_VARS_H_

#ifdef __cplusplus
extern "C" {
#endif

void** __start_brk_location(void);
void** __curr_brk_location(void);

#undef start_brk
#define start_brk (*__start_brk_location())
#undef curr_brk
#define curr_brk (*__curr_brk_location())

#ifdef __cplusplus
}
#endif

#endif
