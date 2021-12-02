set(CMAKE_SYSTEM_NAME Linux)

#set(CMAKE_ASM_COMPILER /usr/local/bin/x86_64-elf-as)
set(CMAKE_C_COMPILER /usr/local/bin/x86_64-elf-gcc)
set(CMAKE_CXX_COMPILER /usr/local/bin/x86_64-elf-g++)
set(CMAKE_AR /usr/local/bin/x86_64-elf-ar)
set(CMAKE_LD /usr/local/bin/x86_64-elf-ld)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

SET(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
# for libraries and headers in the target directories
SET(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
SET(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
