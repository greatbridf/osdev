.code32

.text

.global asm_enable_paging
.type   asm_enable_paging @function
asm_enable_paging:
    cli
    // page directory address
    movl 4(%esp), %eax
    movl %eax, %cr3

    movl %cr0, %eax
    orl $0x80000001, %eax
    movl %eax, %cr0

    ret
