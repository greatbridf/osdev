#ifndef __GBLIBC_BITS_ALLTYPES_H
#define __GBLIBC_BITS_ALLTYPES_H


#define S_IFMT 0170000

#define S_IFSOCK 0140000
#define S_IFLNK 0120000
#define S_IFREG 0100000
#define S_IFBLK 0060000
#define S_IFDIR 0040000
#define S_IFCHR 0020000
#define S_IFIFO 0010000

#define S_ISSOCK(m) (((m)&S_IFMT) == S_IFSOCK)
#define S_ISLNK(m) (((m)&S_IFMT) == S_IFLNK)
#define S_ISREG(m) (((m)&S_IFMT) == S_IFREG)
#define S_ISBLK(m) (((m)&S_IFMT) == S_IFBLK)
#define S_ISDIR(m) (((m)&S_IFMT) == S_IFDIR)
#define S_ISCHR(m) (((m)&S_IFMT) == S_IFCHR)
#define S_ISFIFO(m) (((m)&S_IFMT) == S_IFIFO)

// gblibc extension
#define S_ISBLKCHR(m) (S_ISBLK(m) || S_ISCHR(m))

#ifdef __cplusplus
extern "C" {
#endif

typedef unsigned uid_t;
typedef unsigned gid_t;
typedef unsigned mode_t;

#ifdef __cplusplus
}
#endif

#endif
