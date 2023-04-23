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
    movl %esp, %ebp

    leal (%ebx, %eax, 4), %ebx
    addl $8, %ebx
    pushl %ebx

    pushl %ecx
    pushl %eax

    call __init_gblibc

    subl $4, %ebp

    call main

    movl %eax, %edi  # code
    movl $60, %eax # SYS_exit
    int $0x80        # syscall
