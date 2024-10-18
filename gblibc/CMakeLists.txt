cmake_minimum_required(VERSION 3.15)
project(gblibc)

add_library(gblibc STATIC
    src/stdio.c
    src/arithmetic.c
    src/string.c
    src/fcntl.c
    src/unistd.c
    src/wait.c
    src/assert.c
    src/dirent.c
    src/ctype.c
    src/stdlib.c
    src/errno.c
    src/init.c
    src/internal.c
    src/stat.c
    src/time.c
    src/signal.c
    src/platform-independent.s
)

add_library(gblibc_32 STATIC
    src/stdio.c
    src/arithmetic.c
    src/string.c
    src/fcntl.c
    src/unistd.c
    src/wait.c
    src/assert.c
    src/dirent.c
    src/ctype.c
    src/stdlib.c
    src/errno.c
    src/init.c
    src/internal.c
    src/stat.c
    src/time.c
    src/signal.c
    src/platform-independent.s
)

add_library(crt0_32 OBJECT
    src/crt0.s
)

target_compile_options(gblibc_32 PRIVATE "-m32")
target_compile_options(gblibc_32 PRIVATE "-mcmodel=32")
target_compile_options(crt0_32 PRIVATE "-m32")
target_link_options(gblibc_32 PRIVATE "LINKER:-melf_i386")
target_link_options(crt0_32 PRIVATE "LINKER:-melf_i386")

file(GLOB_RECURSE GBLIBC_PUBLIC_HEADERS ${CMAKE_CURRENT_SOURCE_DIR}/include)

target_include_directories(gblibc PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
                                  PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/private-include)

set_target_properties(gblibc PROPERTIES PRIVATE_HEADER
    "private-include/devutil.h,private-include/syscall.h")
set_target_properties(gblibc PROPERTIES PUBLIC_HEADER "${GBLIBC_PUBLIC_HEADERS}")

target_include_directories(gblibc_32 PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
                                  PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/private-include)

set_target_properties(gblibc_32 PROPERTIES PRIVATE_HEADER
    "private-include/devutil.h,private-include/syscall.h")
set_target_properties(gblibc_32 PROPERTIES PUBLIC_HEADER "${GBLIBC_PUBLIC_HEADERS}")
