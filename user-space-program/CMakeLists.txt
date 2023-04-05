cmake_minimum_required(VERSION 3.15)
project(user_space_program C ASM)

set(CMAKE_C_FLAGS "-nostdlib -nostdinc -static -m32 -W -Wall -Wextra -Werror -mstack-protector-guard=global")
set(CMAKE_ASM_FLAGS "-nostdlib -m32 -static -mstack-protector-guard=global -g0")

link_libraries(gblibc)

set(CMAKE_C_IMPLICIT_LINK_DIRECTORIES "")
set(CMAKE_C_IMPLICIT_LINK_LIBRARIES "")

add_executable(hello-world.out hello-world.s)
add_executable(interrupt-test.out interrupt-test.s)
add_executable(stack-test.out stack-test.s)
add_executable(init.out init.c)
add_executable(sh.out sh.c)
add_executable(priv-test.out priv-test.c)
add_executable(lazybox.out lazybox.c)

add_custom_target(user_space_programs
    DEPENDS hello-world.out interrupt-test.out stack-test.out init.out sh.out priv-test.out
    DEPENDS lazybox.out
)
