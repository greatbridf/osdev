cmake_minimum_required(VERSION 3.15)
project(my_os_bootloader ASM)

set(CMAKE_ASM_FLAGS "-m32")

include_directories(${PROJECT_SOURCE_DIR}/include)

set(EXTRACT_DIR ${PROJECT_BINARY_DIR}/extract)
file(MAKE_DIRECTORY ${EXTRACT_DIR})

set(BOOTLOADER_SOURCES src/boot.s
                       src/asm/a20.s
                       src/asm/interrupt.s
                       src/asm/port_io.s
                       )

add_library(bootloader STATIC ${BOOTLOADER_SOURCES})

add_custom_command(OUTPUT extracted_bootloader
    DEPENDS bootloader
    COMMAND ar xf ${PROJECT_BINARY_DIR}/libbootloader.a --output=${EXTRACT_DIR}
)

project(kernel_main)

set(CMAKE_C_FLAGS "-nostdinc -m32 -nostdlib -Wall -Wextra -fverbose-asm -fno-exceptions -fno-pic -fno-stack-protector")
set(CMAKE_CXX_FLAGS "-nostdinc -m32 -nostdlib -Wall -Wextra -fverbose-asm -fno-use-cxa-atexit -fno-exceptions -fno-pic -fno-stack-protector")

if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O0 -g")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O0 -g")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O")
endif()

include_directories(${PROJECT_SOURCE_DIR}/include)

set(KERNEL_MAIN_SOURCES src/kernel_main.c
                        src/kernel/errno.c
                        src/kernel/interrupt.c
                        src/kernel/stdio.c
                        src/kernel/mem.c
                        src/kernel/vga.c
                        src/kernel/hw/keyboard.c
                        src/types/buffer.c
                        include/asm/boot.h
                        include/asm/port_io.h
                        include/kernel/errno.h
                        include/kernel/interrupt.h
                        include/kernel/stdio.h
                        include/kernel/mem.h
                        include/kernel/vga.h
                        include/kernel/hw/keyboard.h
                        include/kernel/input/keycodes.h
                        include/types/buffer.h
                        include/types/types.h
                        include/types/size.h
                        include/types/stdint.h
                        include/types/list.hpp
                        include/kernel_main.h
                        )
add_library(kernel_main STATIC ${KERNEL_MAIN_SOURCES})

add_custom_command(OUTPUT extracted_kernel_main
    DEPENDS kernel_main
    COMMAND ar xf ${PROJECT_BINARY_DIR}/libkernel_main.a --output=${EXTRACT_DIR}
)

add_custom_target(kernel.out
    DEPENDS extracted_bootloader
    DEPENDS extracted_kernel_main
    COMMAND ld -T ${CMAKE_SOURCE_DIR}/ldscript.ld ${EXTRACT_DIR}/*.o
    -melf_i386 -o ${CMAKE_BINARY_DIR}/kernel.out
)

add_custom_target(boot.img
    DEPENDS kernel.out
    COMMAND objcopy --strip-debug -O binary
    ${CMAKE_BINARY_DIR}/kernel.out boot.img)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
