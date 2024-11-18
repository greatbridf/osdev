#pragma once

#define kmsgf(fmt, ...)
#define kmsg(msg)

#ifdef NDEBUG
#define kmsgf_debug(...)
#else
#define kmsgf_debug(...) kmsgf(__VA_ARGS__)
#endif
