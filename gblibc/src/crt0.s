.code32

.text

# TODO: call .init and .fini, initialize c standard library
.globl _start
.type  _start @function
_start:
    movl (%esp), %eax   # argc
    leal 4(%esp), %ecx  # argv
    movl %esp, %ebx

    andl $0xfffffff0, %esp

    pushl %ebx
    pushl $0

    movl %esp, %ebp

    pushl %ecx
    pushl %eax

    call __init_gblibc

    call main

    movl %eax, %edi  # code
    movl $60, %eax # SYS_exit
    int $0x80        # syscall
