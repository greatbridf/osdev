.code32

.text

# TODO: call .init and .fini, initialize c standard library
.globl _start
.type  _start @function
_start:
    movl (%esp), %eax   # argc
    leal 4(%esp), %ecx  # argv
    movl %esp, %edx

    andl $0xfffffff0, %esp

    pushl %edx
    pushl $0

    movl %esp, %ebp

    pushl %ecx
    pushl %eax

    call main

    movl %eax, %edi  # code
    movl $0x05, %eax # SYS_exit
    int $0x80        # syscall
