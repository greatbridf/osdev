.code32

.text

.globl int13
.type  int13 @function
int13:
    pushal
    call int13_handler
    popal
    iret

.globl irq0
.type  irq0 @function
irq0:
    pushal
    call irq0_handler
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
    sti
    ret
