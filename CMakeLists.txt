cmake_minimum_required(VERSION 3.15)
project(kernel_main C CXX ASM)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(CMAKE_CXX_LINK_EXECUTABLE
    "<CMAKE_LINKER> <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")

set(C_CXX_FLAGS "-nostdinc -nostdlib -W -Wall -Wextra -Wno-stringop-overflow -Wno-builtin-declaration-mismatch -Wno-format -fverbose-asm -fno-exceptions -ffreestanding -fno-pic -mno-red-zone -mstack-protector-guard=global -mcmodel=kernel")
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
add_subdirectory(gblibstdc++)
add_subdirectory(user-space-program)

set(BOOTLOADER_SOURCES src/boot.s
                       src/mbr.S
                       src/asm/interrupt.s
                       )

set(KERNEL_MAIN_SOURCES src/fs/fat.cpp
                        src/fs/procfs.cc
                        src/kinit.cpp
                        src/kernel/async/waitlist.cc
                        src/kernel/async/lock.cc
                        src/kernel/allocator.cc
                        src/kernel/interrupt.cpp
                        src/kernel/process.cpp
                        src/kernel/tty.cpp
                        # src/kernel/syscall.cpp
                        src/kernel/syscall/fileops.cc
                        src/kernel/syscall/mount.cc
                        src/kernel/mem/paging.cc
                        src/kernel/mem/slab.cc
                        src/kernel/mem.cpp
                        src/kernel/module.cc
                        src/kernel/vfs.cpp
                        src/kernel/vga.cpp
                        src/kernel/hw/ahci.cc
                        src/kernel/hw/pci.cc
                        src/kernel/hw/serial.cc
                        src/kernel/hw/timer.cc
                        src/kernel/task/thread.cc
                        src/kernel/task/readyqueue.cc
                        src/kernel/user/thread_local.cc
                        src/kernel/vfs/tmpfs.cc
                        src/kernel/signal.cpp
                        src/types/elf.cpp
                        src/types/libstdcpp.cpp
                        include/fs/fat.hpp
                        include/kernel/async/waitlist.hpp
                        include/kernel/async/lock.hpp
                        include/kernel/tty.hpp
                        include/kernel/interrupt.h
                        include/kernel/irq.hpp
                        include/kernel/process.hpp
                        include/kernel/syscall.hpp
                        include/kernel/mem/paging.hpp
                        include/kernel/mem/slab.hpp
                        include/kernel/mem/types.hpp
                        include/kernel/mm.hpp
                        include/kernel/module.hpp
                        include/kernel/utsname.hpp
                        include/kernel/vfs.hpp
                        include/kernel/vfs/dentry.hpp
                        include/kernel/vfs/file.hpp
                        include/kernel/vfs/inode.hpp
                        include/kernel/vfs/vfs.hpp
                        include/kernel/vga.hpp
                        include/kernel/signal.hpp
                        include/kernel/task/forward.hpp
                        include/kernel/task/thread.hpp
                        include/kernel/task/readyqueue.hpp
                        include/kernel/hw/pci.hpp
                        include/kernel/hw/port.hpp
                        include/kernel/hw/serial.hpp
                        include/kernel/hw/timer.hpp
                        include/kernel/input/keycodes.h
                        include/kernel/user/thread_local.hpp
                        include/types/bitmap.hpp
                        include/types/buffer.hpp
                        include/types/elf.hpp
                        include/types/hash_map.hpp
                        include/types/types.h
                        include/types/allocator.hpp
                        include/types/cplusplus.hpp
                        include/kernel/log.hpp
                        )

add_executable(kernel.out ${KERNEL_MAIN_SOURCES} ${BOOTLOADER_SOURCES})
target_link_libraries(kernel.out gblibc gblibstdc++)
target_include_directories(kernel.out PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_options(kernel.out PRIVATE
    -T ${CMAKE_SOURCE_DIR}/src/kernel.ld -lgblibc -L${CMAKE_BINARY_DIR}/gblibc)
set_target_properties(kernel.out PROPERTIES LINK_DEPENDS ${CMAKE_SOURCE_DIR}/src/kernel.ld)

add_custom_command(OUTPUT mbr_hole.bin
    DEPENDS kernel.out
    COMMAND ${CMAKE_OBJCOPY} --strip-debug -O binary ${CMAKE_BINARY_DIR}/kernel.out mbr_hole.bin
)

add_custom_target(boot.img
    DEPENDS mbr_hole.bin
    DEPENDS user_space_programs
    COMMAND dd if=mbr_hole.bin of=boot.img
    COMMAND dd if=/dev/zero of=boot.img bs=`expr 512 \\* 1024 \\* 1024` count=0 seek=1
    COMMAND sh -c \"echo n\; echo\; echo\; echo\; echo\; echo a\; echo w\" | ${FDISK_BIN} boot.img
    COMMAND mkfs.fat --offset=2048 -v -n SYSTEM boot.img
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/hello-world.out ::hello
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/interrupt-test.out ::int
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/stack-test.out ::stack
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/init.out ::init
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/sh.out ::sh
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/priv-test.out ::priv
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/lazybox.out ::lazybox
    COMMAND mcopy -i boot.img@@1M ${CMAKE_BINARY_DIR}/user-space-program/lazybox.out ::pwd
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/busybox-minimal ::busybox_
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/busybox ::busybox
    COMMAND mcopy -i boot.img@@1M ${CMAKE_SOURCE_DIR}/init_script.sh ::initsh
)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
