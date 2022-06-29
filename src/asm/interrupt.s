.code32

.text

# TODO: stack alignment
.globl int6
.type  int6 @function
int6:
    pushal
    call int6_handler
    popal

    iret

# TODO: stack alignment
.globl int8
.type  int8 @function
int8:
    nop
    iret

# TODO: stack alignment
.globl int13
.type  int13 @function
int13:
    pushal
    call int13_handler
    popal

# remove the 32bit error code from stack
    addl $4, %esp
    iret

.globl int14
.type  int14 @function
int14:
    pushal
    movl %cr2, %eax
    pushl %eax

    # stack alignment and push *data
    movl %esp, %eax
    subl $0x4, %esp
    andl $0xfffffff0, %esp
    movl %eax, (%esp)

    call int14_handler

    # restore stack
    popl %esp

    popl %eax
    popal

# remove the 32bit error code from stack
    addl $4, %esp
    iret

.globl irq0
.type  irq0 @function
irq0:
    pushal

    # stack alignment and push *data
    movl %esp, %eax
    subl $0x4, %esp
    andl $0xfffffff0, %esp
    movl %eax, (%esp)

    call irq0_handler

    # restore stack
    popl %esp

    popal
    iret

.globl irq1
.type  irq1 @function
irq1:
    pushal
    call irq1_handler
    popal
    iret

.globl irq2
.type  irq2 @function
irq2:
    pushal
    call irq2_handler
    popal
    iret
.globl irq3
.type  irq3 @function
irq3:
    pushal
    call irq3_handler
    popal
    iret
.globl irq4
.type  irq4 @function
irq4:
    pushal
    call irq4_handler
    popal
    iret
.globl irq5
.type  irq5 @function
irq5:
    pushal
    call irq5_handler
    popal
    iret
.globl irq6
.type  irq6 @function
irq6:
    pushal
    call irq6_handler
    popal
    iret
.globl irq7
.type  irq7 @function
irq7:
    pushal
    call irq7_handler
    popal
    iret
.globl irq8
.type  irq8 @function
irq8:
    pushal
    call irq8_handler
    popal
    iret
.globl irq9
.type  irq9 @function
irq9:
    pushal
    call irq9_handler
    popal
    iret
.globl irq10
.type  irq10 @function
irq10:
    pushal
    call irq10_handler
    popal
    iret
.globl irq11
.type  irq11 @function
irq11:
    pushal
    call irq11_handler
    popal
    iret
.globl irq12
.type  irq12 @function
irq12:
    pushal
    call irq12_handler
    popal
    iret
.globl irq13
.type  irq13 @function
irq13:
    pushal
    call irq13_handler
    popal
    iret
.globl irq14
.type  irq14 @function
irq14:
    pushal
    call irq14_handler
    popal
    iret
.globl irq15
.type  irq15 @function
irq15:
    pushal
    call irq15_handler
    popal
    iret

.globl asm_load_idt
.type  asm_load_idt @function
asm_load_idt:
    movl 4(%esp), %edx
    lidt (%edx)
    movl 8(%esp), %edx
    cmpl $0, %edx
    je asm_load_idt_skip
    sti
asm_load_idt_skip:
    ret

.globl syscall_stub
.type  syscall_stub @function
syscall_stub:
    cmpl $8, %eax
    jge syscall_stub_end
    pushal

    # syscall function no. is in %eax
    # store it in %ebx
    movl %eax, %ebx
    # stack alignment and push *data
    movl %esp, %eax
    subl $0x4, %esp
    andl $0xfffffff0, %esp
    movl %eax, (%esp)

    call *syscall_handlers(,%ebx,4)

    # restore stack
    popl %esp

    popal
syscall_stub_end:
    iret

# parameters:
# interrupt_stack* ret_stack
.globl to_kernel
.type  to_kernel @function
to_kernel:
    movw $0x10, %ax
    movw %ax, %ss
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs

    movl 4(%esp), %eax

    movl (%eax), %edi
    movl 4(%eax), %esi
    movl 8(%eax), %ebp
    movl 12(%eax), %esp # %esp is the dst stack
    movl 16(%eax), %ebx
    movl 20(%eax), %edx
    movl 24(%eax), %ecx
#   TODO: optimize for caching
#   movl 28(%eax), %eax # we deal with %eax later

    pushl 40(%eax) # %eflags
    pushl $0x08    # %cs
    pushl 32(%eax) # %eip

    movl 28(%eax), %eax

    iret

# parameters:
# interrupt_stack* ret_stack
.globl to_user
.type  to_user @function
to_user:
    movw $0x23, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs

    movl 4(%esp), %edi

    movl 40(%edi), %ebp # save eflags
    movl 32(%edi), %esi # save eip

    movl 28(%edi), %eax
    movl 24(%edi), %ecx
    movl 20(%edi), %edx
    movl 16(%edi), %ebx

    pushl $0x23    # %ss
    pushl 12(%edi) # %esp
    pushl %ebp     # %eflags
    pushl $0x1b    # %cs
    pushl %esi     # %eip

    movl 8(%edi), %ebp
    movl 4(%edi), %esi
    movl (%edi), %edi

    iret
