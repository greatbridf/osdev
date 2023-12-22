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

    # allocate space for mmx registers and argument
    subl $0x210, %esp

    # align stack to 16byte boundary
    and $0xfffffff0, %esp

    # save mmx registers
    fxsave 16(%esp)

    # push (interrupt_stack*)data
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
irq0:
    pushal
    mov $0, %eax
    jmp irqstub
.globl irq1
irq1:
    pushal
    mov $1, %eax
    jmp irqstub
.globl irq2
irq2:
    pushal
    mov $2, %eax
    jmp irqstub
.globl irq3
irq3:
    pushal
    mov $3, %eax
    jmp irqstub
.globl irq4
irq4:
    pushal
    mov $4, %eax
    jmp irqstub
.globl irq5
irq5:
    pushal
    mov $5, %eax
    jmp irqstub
.globl irq6
irq6:
    pushal
    mov $6, %eax
    jmp irqstub
.globl irq7
irq7:
    pushal
    mov $7, %eax
    jmp irqstub
.globl irq8
irq8:
    pushal
    mov $8, %eax
    jmp irqstub
.globl irq9
irq9:
    pushal
    mov $9, %eax
    jmp irqstub
.globl irq10
irq10:
    pushal
    mov $10, %eax
    jmp irqstub
.globl irq11
irq11:
    pushal
    mov $11, %eax
    jmp irqstub
.globl irq12
irq12:
    pushal
    mov $12, %eax
    jmp irqstub
.globl irq13
irq13:
    pushal
    mov $13, %eax
    jmp irqstub
.globl irq14
irq14:
    pushal
    mov $14, %eax
    jmp irqstub
.globl irq15
irq15:
    pushal
    mov $15, %eax
    jmp irqstub

.globl irqstub
irqstub:
    # save current esp
    mov %esp, %ebx

    # align stack to 16byte boundary
    and $0xfffffff0, %esp

    # save mmx registers
    subl $512, %esp
    fxsave (%esp)

    # push irq number
    sub $16, %esp
    mov %eax, (%esp)

    call irq_handler

    # restore mmx registers
    fxrstor 16(%esp)

    # restore stack and general purpose registers
    mov %ebx, %esp
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
