cmake_minimum_required(VERSION 3.15)
project(kernel_main C CXX ASM)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

set(CMAKE_CXX_LINK_EXECUTABLE
    "<CMAKE_LINKER> <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>")

set(C_CXX_FLAGS "-nostdinc -nostdlib -mno-sse -mno-mmx -W -Wall -Wextra -Wno-stringop-overflow -Wno-builtin-declaration-mismatch -Wno-format -fverbose-asm -fno-exceptions -ffreestanding -fno-pic -mno-red-zone -mstack-protector-guard=global -mcmodel=kernel")
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

set(BOOTLOADER_SOURCES src/boot.s
                       src/mbr.S
                       )

set(KERNEL_MAIN_SOURCES include/kernel/async/lock.hpp
                        include/kernel/mem/paging.hpp
                        include/kernel/mem/slab.hpp
                        include/types/list.hpp
                        include/types/types.h
                        include/types/allocator.hpp
                        )

add_executable(kernel.out ${KERNEL_MAIN_SOURCES} ${BOOTLOADER_SOURCES})
add_dependencies(kernel.out rustpart)
target_link_libraries(kernel.out gblibc gblibstdc++ gbos_rust_part)
target_include_directories(kernel.out PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_options(kernel.out PRIVATE
    -T "${CMAKE_SOURCE_DIR}/src/kernel.ld"
    -L "${CMAKE_BINARY_DIR}/x86_64-unknown-none/${CARGO_BUILD_TYPE}"
    --no-check-sections
    )
set_target_properties(kernel.out PROPERTIES LINK_DEPENDS "${CMAKE_SOURCE_DIR}/src/kernel.ld")
set_source_files_properties(src/mbr.S PROPERTIES OBJECT_DEPENDS
    "${CMAKE_BINARY_DIR}/x86_64-unknown-none/${CARGO_BUILD_TYPE}/libgbos_rust_part.a"
    )

add_custom_target(rustpart
    COMMAND cargo build ${CARGO_BUILD_FLAGS}
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
