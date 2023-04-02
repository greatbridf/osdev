cmake_minimum_required(VERSION 3.15)
project(kernel_main C CXX ASM)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(CMAKE_CXX_LINK_EXECUTABLE
    "<CMAKE_LINKER> <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")

set(CMAKE_ASM_FLAGS "-m32")
set(C_CXX_FLAGS "-nostdinc -m32 -nostdlib -W -Wall -Wextra -Wno-builtin-declaration-mismatch -Wno-format -fverbose-asm -fno-exceptions -ffreestanding -fno-pic -mstack-protector-guard=global")
set(CMAKE_C_FLAGS "${C_CXX_FLAGS} -Werror=implicit-int -Werror=implicit-function-declaration -Werror=strict-aliasing")
set(CMAKE_CXX_FLAGS "${C_CXX_FLAGS} -fno-use-cxa-atexit -fno-rtti")
set(CMAKE_CXX_LINK_FLAGS "")
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

add_subdirectory(gblibc)
add_subdirectory(user-space-program)

set(BOOTLOADER_SOURCES src/boot.s
                       src/asm/interrupt.s
                       src/asm/port_io.s
                       src/asm/sys.s
                       )

set(KERNEL_MAIN_SOURCES src/fs/fat.cpp
                        src/kinit.cpp
                        src/kernel/errno.c
                        src/kernel/interrupt.cpp
                        src/kernel/process.cpp
                        src/kernel/tty.cpp
                        src/kernel/syscall.cpp
                        src/kernel/mem.cpp
                        src/kernel/vfs.cpp
                        src/kernel/vga.cpp
                        src/kernel/hw/ata.cpp
                        src/kernel/hw/keyboard.cpp
                        src/kernel/hw/serial.cpp
                        src/kernel/hw/timer.c
                        src/kernel/event/event.cpp
                        src/kernel/signal.cpp
                        src/types/bitmap.c
                        src/types/elf.cpp
                        src/types/libstdcpp.cpp
                        include/asm/port_io.h
                        include/asm/sys.h
                        include/fs/fat.hpp
                        include/kernel/event/event.h
                        include/kernel/event/evtqueue.hpp
                        include/kernel/errno.h
                        include/kernel/tty.hpp
                        include/kernel/interrupt.h
                        include/kernel/process.hpp
                        include/kernel/syscall.hpp
                        include/kernel/mem.h
                        include/kernel/mm.hpp
                        include/kernel/vfs.hpp
                        include/kernel/vga.hpp
                        include/kernel/signal.hpp
                        include/kernel/hw/ata.hpp
                        include/kernel/hw/keyboard.h
                        include/kernel/hw/port.hpp
                        include/kernel/hw/serial.h
                        include/kernel/hw/timer.h
                        include/kernel/input/keycodes.h
                        include/kernel/input/input_event.h
                        include/types/bitmap.h
                        include/types/buffer.hpp
                        include/types/elf.hpp
                        include/types/hash_map.hpp
                        include/types/map.hpp
                        include/types/pair.hpp
                        include/types/types.h
                        include/types/size.h
                        include/types/status.h
                        include/types/allocator.hpp
                        include/types/cplusplus.hpp
                        include/types/list.hpp
                        include/types/lock.hpp
                        include/types/string.hpp
                        include/types/vector.hpp
                        include/types/function.hpp
                        include/kernel/log.hpp
                        )

add_executable(kernel.out ${KERNEL_MAIN_SOURCES} ${BOOTLOADER_SOURCES})
target_link_libraries(kernel.out gblibc)
target_include_directories(kernel.out PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_options(kernel.out PRIVATE
    -T ${CMAKE_SOURCE_DIR}/src/kernel.ld -melf_i386 -lgblibc -L${CMAKE_BINARY_DIR}/gblibc)
set_target_properties(kernel.out PROPERTIES LINK_DEPENDS ${CMAKE_SOURCE_DIR}/src/kernel.ld)

add_custom_command(OUTPUT mbr.bin
    DEPENDS ${PROJECT_SOURCE_DIR}/src/mbr.S ${PROJECT_SOURCE_DIR}/src/mbr.ld
    COMMAND ${CMAKE_ASM_COMPILER} -m32 -c ${PROJECT_SOURCE_DIR}/src/mbr.S -o mbr.o
    COMMAND ${CMAKE_LINKER} -T ${PROJECT_SOURCE_DIR}/src/mbr.ld mbr.o -o mbr.bin
)

add_custom_command(OUTPUT mbr_hole.bin
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
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/hello-world.out ::hello.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/interrupt-test.out ::int.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/stack-test.out ::stack.out
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/init.out ::init.elf
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/sh.out ::sh.elf
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/priv-test.out ::priv.elf
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/lazybox.out ::lazybox
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/lazybox.out ::pwd
)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
