.code32

.text

# TODO: call .init and .fini, initialize c standard library
.globl _start
.type  _start @function
_start:
    movl %esp, %ebx     # initial stack
    andl $0xfffffff0, %esp
    pushl $0
    movl %esp, %ebp

    movl (%ebx), %eax           # %eax = argc

    leal 8(%ebx, %eax, 4), %ecx # %ecx = envp
    pushl %ecx

    leal 4(%ebx), %ecx          # %ecx = argv
    pushl %ecx

    pushl %eax

    call __init_gblibc

    movl (%ebx), %eax # %eax = argc
    movl %eax, (%esp)
    leal 4(%ebx), %eax
    movl %eax, 4(%esp)

    call main

    movl %eax, %edi  # code
    movl $60, %eax   # SYS_exit
    int $0x80        # syscall
