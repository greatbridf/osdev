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
    # push general purpose registers
    pushal

    # save %cr2
    movl %cr2, %eax
    pushl %eax

    # save current esp (also pointer to struct int14_data)
    mov %esp, %ebx

    # align stack to 16byte boundary
    and $0xfffffff0, %esp

    # save mmx registers
    subl $512, %esp
    fxsave (%esp)

    # push *data
    sub $16, %esp
    mov %ebx, (%esp)

    call int14_handler

    # restore mmx registers
    fxrstor 16(%esp)

    # restore stack and general purpose registers
    leal 4(%ebx), %esp
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

.globl syscall_stub
.type  syscall_stub @function
syscall_stub:
    pushal

    # stack alignment and push *data
    movl %esp, %eax
    subl $0x4, %esp
    andl $0xfffffff0, %esp
    movl %eax, (%esp)

    call syscall_entry

    # restore stack
    popl %esp

.globl _syscall_stub_fork_return
.type  _syscall_stub_fork_return @function
_syscall_stub_fork_return:
    popal
    iret

# parameters
# #1: uint32_t* curr_esp
# #2: uint32_t next_esp
.globl asm_ctx_switch
.type  asm_ctx_switch @function
asm_ctx_switch:
    movl 4(%esp), %ecx
    movl 8(%esp), %eax

    push $_ctx_switch_return
    push %ebx
    push %edi
    push %esi
    push %ebp
    pushfl

    movl %esp, (%ecx)
    movl %eax, %esp

    popfl
    pop %ebp
    pop %esi
    pop %edi
    pop %ebx

    ret

_ctx_switch_return:
    ret

.section .text.kinit

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
