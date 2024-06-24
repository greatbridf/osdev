.text

ISR_stub:
	sub $0x78, %rsp
	mov %rax,  0x00(%rsp)
	mov %rbx,  0x08(%rsp)
	mov %rcx,  0x10(%rsp)
	mov %rdx,  0x18(%rsp)
	mov %rdi,  0x20(%rsp)
	mov %rsi,  0x28(%rsp)
	mov %r8,   0x30(%rsp)
	mov %r9,   0x38(%rsp)
	mov %r10,  0x40(%rsp)
	mov %r11,  0x48(%rsp)
	mov %r12,  0x50(%rsp)
	mov %r13,  0x58(%rsp)
	mov %r14,  0x60(%rsp)
	mov %r15,  0x68(%rsp)
	mov %rbp,  0x70(%rsp)

	mov 0x78(%rsp), %rax
	sub $ISR0, %rax
	shr $3, %rax
	mov %rax, 0x78(%rsp)

	mov %rsp, %rbx
	and $~0xf, %rsp

	sub $512, %rsp
	fxsave (%rsp)

	mov %rbx, %rdi
	mov %rsp, %rsi
	call interrupt_handler

	fxrstor (%rsp)
	mov %rbx, %rsp

	mov 0x00(%rsp), %rax
	mov 0x08(%rsp), %rbx
	mov 0x10(%rsp), %rcx
	mov 0x18(%rsp), %rdx
	mov 0x20(%rsp), %rdi
	mov 0x28(%rsp), %rsi
	mov 0x30(%rsp), %r8
	mov 0x38(%rsp), %r9
	mov 0x40(%rsp), %r10
	mov 0x48(%rsp), %r11
	mov 0x50(%rsp), %r12
	mov 0x58(%rsp), %r13
	mov 0x60(%rsp), %r14
	mov 0x68(%rsp), %r15
	mov 0x70(%rsp), %rbp

	mov 0x78(%rsp), %rsp
	iretq

.globl syscall_stub
.type  syscall_stub @function
syscall_stub:
    # pushal

    # save current esp
    mov %esp, %ebx

    # stack alignment
    and $0xfffffff0, %esp

    # save mmx registers
    sub $(512 + 16), %esp
    fxsave 16(%esp)

    # save pointers to context and mmx registers
    mov %ebx, (%esp) # pointer to context
    lea 16(%esp), %eax
    mov %eax, 4(%esp) # pointer to mmx registers

    # TODO: LONG MODE
    # call syscall_entry

    # restore mmx registers
    fxrstor 16(%esp)

    # restore stack
    mov %ebx, %esp

.globl _syscall_stub_fork_return
.type  _syscall_stub_fork_return @function
_syscall_stub_fork_return:
    # popal
    iretq

# parameters
# #1: esp* curr_esp
# #2: esp* next_esp
.globl asm_ctx_switch
.type  asm_ctx_switch @function
asm_ctx_switch:
    movl 4(%esp), %ecx
    movl 8(%esp), %eax

    pushq $_ctx_switch_return
    push %rbx
    push %rdi
    push %rsi
    push %rbp
    pushf

    # push esp to restore
    push (%rcx)

    mov %esp, (%rcx)
    mov (%rax), %esp

    # restore esp
    pop (%rax)

    popf
    pop %rbp
    pop %rsi
    pop %rdi
    pop %rbx

    ret

_ctx_switch_return:
    ret

.altmacro
.macro build_isr name
	.align 8
	ISR\name:
		call ISR_stub
.endm

.set i, 0
.rept 48
	build_isr %i
	.set i, i+1
.endr

.section .rodata

.align 8
.globl ISR_START_ADDR
.type  ISR_START_ADDR @object
ISR_START_ADDR:
	.quad ISR0
