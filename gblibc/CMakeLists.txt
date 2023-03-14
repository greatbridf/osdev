cmake_minimum_required(VERSION 3.15)
project(gblibc)

add_library(gblibc STATIC
    src/stdio.c
    src/arithmetic.c
    src/string.c
    src/crt0.s
)

target_include_directories(gblibc PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
                                  PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/private-include)

set_target_properties(gblibc PROPERTIES PRIVATE_HEADER
    "private-include/devutil.h")
set_target_properties(gblibc PROPERTIES PUBLIC_HEADER
    "include/stdio.h,include/stdint.h,include/stdarg.h,include/string.h")
