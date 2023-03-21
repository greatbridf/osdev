.section .text.kinit

.globl check_a20_on
.type  check_a20_on @function

check_a20_on:
    pushal
    movl $0x112345, %edi
    movl $0x012345, %esi

    movl (%esi), %eax
    movl (%edi), %ecx

    movl %esi, (%esi)
    movl %edi, (%edi)
    cmpsl

    subl $4, %esi
    subl $4, %edi

    movl %eax, (%esi)
    movl %ecx, (%edi)

    popal
    jne a20_on
    movl $0, %eax
    ret
a20_on:
    movl $1, %eax
    ret
