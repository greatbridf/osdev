.text

.globl check_a20_on
.type  check_a20_on @function

check_a20_on:
    pushal
    movl $0x112345, %edi
    movl $0x012345, %esi
    movl %esi, (%esi)
    movl %edi, (%edi)
    cmpsd
    popal
    jne a20_on
    movl $0, %eax
    ret
a20_on:
    movl $1, %eax
    ret
