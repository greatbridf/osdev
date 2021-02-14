cmake_minimum_required(VERSION 3.15)
project(my_os_bootloader ASM)

set(CMAKE_CXX_FLAGS "-nostdinc -m32 -nostdlib")
set(CMAKE_ASM_FLAGS "-m32")

set(EXTRACT_DIR ${PROJECT_BINARY_DIR}/extract)
file(MAKE_DIRECTORY ${EXTRACT_DIR})

set(BOOTLOADER_SOURCES src/boot.s
                       )

add_library(bootloader STATIC ${BOOTLOADER_SOURCES})

add_custom_command(OUTPUT extracted_bootloader
    DEPENDS bootloader
    COMMAND ar xf ${PROJECT_BINARY_DIR}/libbootloader.a --output=${EXTRACT_DIR}
)

add_custom_target(boot.img
    DEPENDS extracted_bootloader
    COMMAND ld -T ${CMAKE_SOURCE_DIR}/ldscript.ld ${EXTRACT_DIR}/*.o
    -melf_i386 --oformat=binary -o ${CMAKE_BINARY_DIR}/boot.img
)

add_custom_command(OUTPUT run
    POST_BUILD
    DEPENDS boot.img
    COMMAND bochs -f ${CMAKE_SOURCE_DIR}/bochs.conf
)
