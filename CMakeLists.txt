cmake_minimum_required(VERSION 3.15)
project(kernel_main C CXX ASM)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(NOT DEFINED ARCH)
    set(ARCH "x86_64" CACHE STRING "Target architecture")
endif()
message(STATUS "Building for architecture: ${ARCH}")

if(ARCH STREQUAL "x86_64")
    set(ARCH_C_CXX_FLAGS "-mno-sse -mno-mmx -mno-red-zone -mcmodel=kernel")
    set(ARCH_ASM_FILES arch/src/x86_64/boot.s)
    set(ARCH_LINKER_SCRIPT "${CMAKE_SOURCE_DIR}/arch/src/x86_64/kernel.ld")
    set(RUST_TARGET "x86_64-unknown-none")
elseif(ARCH STREQUAL "riscv64")
    set(ARCH_C_CXX_FLAGS "-march=rv64gc")
    set(ARCH_ASM_FILES arch/src/riscv64/boot.s)
    set(ARCH_LINKER_SCRIPT "${CMAKE_SOURCE_DIR}/arch/src/riscv64/kernel.ld")
    set(RUST_TARGET "riscv64gc-unknown-none-elf")
else()
    message(FATAL_ERROR "Unsupported architecture: ${ARCH}")
endif()

set(CMAKE_CXX_LINK_EXECUTABLE
    "<CMAKE_LINKER> <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")

set(C_CXX_FLAGS "-nostdinc -nostdlib ${ARCH_C_CXX_FLAGS} -W -Wall -Wextra -Wno-stringop-overflow -Wno-builtin-declaration-mismatch -Wno-format -fverbose-asm -fno-exceptions -ffreestanding -fno-pic -mstack-protector-guard=global")
set(CMAKE_C_FLAGS "${C_CXX_FLAGS} -Werror=implicit-int -Werror=implicit-function-declaration -Werror=strict-aliasing")
set(CMAKE_CXX_FLAGS "${C_CXX_FLAGS} -fno-use-cxa-atexit -fno-rtti")
set(CMAKE_CXX_LINK_FLAGS "-z noexecstack --gc-sections")
SET(CMAKE_ASM_FLAGS "${CFLAGS} -x assembler-with-cpp")
set(CMAKE_CXX_STANDARD 20)

if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -DDEBUG -O0 -g")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DDEBUG -O0 -g")
    set(CARGO_BUILD_TYPE "debug")
    set(CARGO_BUILD_FLAGS "")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O2 -g -DNDEBUG")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O2 -g -DNDEBUG")
    set(CARGO_BUILD_TYPE "release")
    set(CARGO_BUILD_FLAGS "--release")
endif()

if (NOT DEFINED FDISK_BIN)
    set(FDISK_BIN fdisk)
endif()

add_subdirectory(gblibc)
add_subdirectory(gblibstdc++)
add_subdirectory(user-space-program)

if(ARCH STREQUAL "x86_64")
    set(BOOTLOADER_SOURCES arch/src/x86_64/boot.s
                           arch/src/x86_64/mbr.S
                          )
elseif(ARCH STREQUAL "riscv64")
    set(BOOTLOADER_SOURCES arch/src/riscv64/boot.s
                           arch/src/riscv64/entry.S
                          )
endif()

#[[set(KERNEL_ARCH_SOURCES "")
if(ARCH STREQUAL "x86_64")
    set(KERNEL_ARCH_SOURCES arch/src/x86_64/init.cpp
                            arch/src/x86_64/interrupt.cpp
                            # more
                           )
elseif(ARCH STREQUAL "riscv64")
    set(KERNEL_ARCH_SOURCES arch/src/riscv64/init.cpp
                            arch/src/riscv64/trap.cpp
                            # more
                           )
endif()]]

set(KERNEL_MAIN_SOURCES src/kinit.cpp
                        src/kernel/async/lock.cc
                        src/kernel/allocator.cc
                        src/kernel/mem/slab.cc
                        src/kernel/hw/acpi.cc
                        src/kernel/hw/pci.cc
                        src/types/libstdcpp.cpp
                        include/defs.hpp
                        include/kernel/async/lock.hpp
                        include/kernel/mem/paging.hpp
                        include/kernel/mem/slab.hpp
                        include/kernel/mem/types.hpp
                        include/kernel/utsname.hpp
                        include/kernel/hw/acpi.hpp
                        include/kernel/hw/pci.hpp
                        include/kernel/hw/port.hpp
                        include/types/list.hpp
                        include/types/types.h
                        include/types/allocator.hpp
                        include/kernel/log.hpp
                        #${KERNEL_ARCH_SOURCES}
                        )

add_executable(kernel.out ${KERNEL_MAIN_SOURCES} ${BOOTLOADER_SOURCES})
add_dependencies(kernel.out rustpart)
target_link_libraries(kernel.out gblibc gblibstdc++ gbos_rust_part)
target_include_directories(kernel.out PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_options(kernel.out PRIVATE
    -T "${ARCH_LINKER_SCRIPT}"
    -L "${CMAKE_BINARY_DIR}/${RUST_TARGET}/${CARGO_BUILD_TYPE}"
    --no-check-sections
    )
set_target_properties(kernel.out PROPERTIES LINK_DEPENDS "${ARCH_LINKER_SCRIPT}")

if(ARCH STREQUAL "x86_64")
    set_source_files_properties(arch/src/x86_64/mbr.S PROPERTIES OBJECT_DEPENDS
        "${CMAKE_BINARY_DIR}/${RUST_TARGET}/${CARGO_BUILD_TYPE}/libgbos_rust_part.a"
        )
endif()

add_custom_target(rustpart
    COMMAND cargo build ${CARGO_BUILD_FLAGS} --target=${RUST_TARGET}
)

add_custom_command(OUTPUT mbr_hole.bin
    DEPENDS kernel.out
    COMMAND ${CMAKE_OBJCOPY} --strip-debug -O binary ${CMAKE_BINARY_DIR}/kernel.out mbr_hole.bin
)

add_custom_target(boot.img
    DEPENDS mbr_hole.bin
    DEPENDS user_space_programs
    COMMAND make -C ${CMAKE_SOURCE_DIR} image
)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
