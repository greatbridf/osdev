cmake_minimum_required(VERSION 3.15)
project(my_os_bootloader ASM)

set(CMAKE_ASM_FLAGS "-m32")

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include_directories(${PROJECT_SOURCE_DIR}/include)

set(EXTRACT_DIR ${PROJECT_BINARY_DIR}/extract)
file(MAKE_DIRECTORY ${EXTRACT_DIR})

set(BOOTLOADER_SOURCES src/boot.s
                       src/asm/a20.s
                       src/asm/interrupt.s
                       src/asm/port_io.s
                       src/asm/sys.s
                       )

add_library(bootloader STATIC ${BOOTLOADER_SOURCES})

add_custom_command(OUTPUT mbr.bin
    DEPENDS ${PROJECT_SOURCE_DIR}/src/mbr.S ${PROJECT_SOURCE_DIR}/src/mbr.ld
    COMMAND ${CMAKE_ASM_COMPILER} -m32 -c ${PROJECT_SOURCE_DIR}/src/mbr.S -o mbr.o
    COMMAND ${CMAKE_LINKER} -T ${PROJECT_SOURCE_DIR}/src/mbr.ld mbr.o -o mbr.bin
)

add_custom_command(OUTPUT extracted_bootloader
    DEPENDS bootloader
    COMMAND ${CMAKE_AR} xf ${PROJECT_BINARY_DIR}/libbootloader.a --output=${EXTRACT_DIR}
)

project(kernel_main)

set(CMAKE_C_FLAGS "-nostdinc -m32 -nostdlib -W -Wall -Wextra -Wno-builtin-declaration-mismatch -Wno-format -Werror=implicit-int -Werror=implicit-function-declaration -Werror=strict-aliasing -fverbose-asm -fno-exceptions -fno-pic -fno-stack-protector")
set(CMAKE_CXX_FLAGS "-nostdinc -m32 -nostdlib -W -Wall -Wextra -Wno-builtin-declaration-mismatch -Wno-format -fverbose-asm -fno-use-cxa-atexit -fno-exceptions -fno-pic -fno-stack-protector -fno-rtti")
set(CMAKE_CXX_STANDARD 20)

if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O0 -g")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O0 -g")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O2")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O2")
endif()

if (NOT DEFINED FDISK_BIN)
    set(FDISK_BIN fdisk)
endif()

set(KERNEL_MAIN_SOURCES src/fs/fat.cpp
                        src/kernel_main.c
                        src/kernel/errno.c
                        src/kernel/interrupt.cpp
                        src/kernel/process.cpp
                        src/kernel/tty.c
                        src/kernel/stdio.c
                        src/kernel/syscall.cpp
                        src/kernel/mem.cpp
                        src/kernel/vfs.cpp
                        src/kernel/vga.c
                        src/kernel/hw/ata.cpp
                        src/kernel/hw/keyboard.cpp
                        src/kernel/hw/serial.c
                        src/kernel/hw/timer.c
                        src/kernel/event/event.cpp
                        src/types/bitmap.c
                        src/types/buffer.c
                        src/types/elf.cpp
                        include/asm/boot.h
                        include/asm/port_io.h
                        include/asm/sys.h
                        include/fs/fat.hpp
                        include/kernel/event/event.h
                        include/kernel/errno.h
                        include/kernel/tty.h
                        include/kernel/interrupt.h
                        include/kernel/process.hpp
                        include/kernel/stdio.h
                        include/kernel/syscall.hpp
                        include/kernel/mem.h
                        include/kernel/mm.hpp
                        include/kernel/vfs.hpp
                        include/kernel/vga.h
                        include/kernel/hw/ata.hpp
                        include/kernel/hw/keyboard.h
                        include/kernel/hw/port.hpp
                        include/kernel/hw/serial.h
                        include/kernel/hw/timer.h
                        include/kernel/input/keycodes.h
                        include/kernel/input/input_event.h
                        include/types/assert.h
                        include/types/bitmap.h
                        include/types/buffer.h
                        include/types/elf.hpp
                        include/types/hash_map.hpp
                        include/types/types.h
                        include/types/size.h
                        include/types/status.h
                        include/types/stdint.h
                        include/types/allocator.hpp
                        include/types/cplusplus.hpp
                        include/types/list.hpp
                        include/types/lock.hpp
                        include/types/string.hpp
                        include/types/vector.hpp
                        include/kernel_main.h
                        )
add_library(kernel_main STATIC ${KERNEL_MAIN_SOURCES})

add_custom_command(OUTPUT extracted_kernel_main
    DEPENDS kernel_main
    COMMAND ${CMAKE_AR} xf ${PROJECT_BINARY_DIR}/libkernel_main.a --output=${EXTRACT_DIR}
)

add_custom_target(user_space_programs
    COMMAND cp ${CMAKE_SOURCE_DIR}/user-space-program/Makefile.src ${CMAKE_SOURCE_DIR}/user-space-program/Makefile
    COMMAND make -C ${CMAKE_SOURCE_DIR}/user-space-program CROSS_COMPILE=${TOOLCHAIN_PATH_AND_PREFIX} all
)

add_custom_target(kernel.out
    DEPENDS extracted_bootloader
    DEPENDS extracted_kernel_main
    DEPENDS user_space_programs
    DEPENDS ${CMAKE_SOURCE_DIR}/src/kernel.ld
    COMMAND ${CMAKE_LINKER} -T ${CMAKE_SOURCE_DIR}/src/kernel.ld ${EXTRACT_DIR}/*.o
    -melf_i386 -o ${CMAKE_BINARY_DIR}/kernel.out
)

add_custom_target(mbr_hole.bin
    DEPENDS kernel.out
    COMMAND ${CMAKE_OBJCOPY} --strip-debug -O binary ${CMAKE_BINARY_DIR}/kernel.out mbr_hole.bin
)

add_custom_target(boot.img
    DEPENDS mbr.bin
    DEPENDS mbr_hole.bin
    DEPENDS user_space_programs
    COMMAND dd if=mbr.bin of=boot.img
    COMMAND cat mbr_hole.bin >> boot.img
    COMMAND dd if=/dev/zero of=boot.img bs=`expr 512 \\* 1024 \\* 1024` count=0 seek=1
    COMMAND sh -c \"echo n\; echo\; echo\; echo\; echo\; echo a\; echo w\" | ${FDISK_BIN} boot.img
    COMMAND mkfs.fat --offset=2048 -v -n SYSTEM boot.img
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/user-space-program/build/hello-world.out ::hello.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/user-space-program/build/interrupt-test.out ::int.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/user-space-program/build/stack-test.out ::stack.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/user-space-program/build/init.out ::init.elf
)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
