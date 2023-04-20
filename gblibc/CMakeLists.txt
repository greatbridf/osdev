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
)

add_library(crt0 OBJECT
    src/crt0.s
)

file(GLOB_RECURSE GBLIBC_PUBLIC_HEADERS ${CMAKE_CURRENT_SOURCE_DIR}/include)

target_include_directories(gblibc PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
                                  PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/private-include)

set_target_properties(gblibc PROPERTIES PRIVATE_HEADER
    "private-include/devutil.h,private-include/syscall.h")
set_target_properties(gblibc PROPERTIES PUBLIC_HEADER "${GBLIBC_PUBLIC_HEADERS}")
