.text

.extern after_ctx_switch
.globl ISR_stub_restore

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

ISR_stub_restore:
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

# parameters
# #1: sp* current_task_sp
# #2: sp* target_task_sp
.globl asm_ctx_switch
.type  asm_ctx_switch @function
asm_ctx_switch:
    pushf
	sub $0x38, %rsp  # extra 8 bytes to align to 16 bytes

    mov %rbx, 0x08(%rsp)
	mov %rbp, 0x10(%rsp)
	mov %r12, 0x18(%rsp)
	mov %r13, 0x20(%rsp)
	mov %r14, 0x28(%rsp)
	mov %r15, 0x30(%rsp)

    push (%rdi) 	 # save sp of previous stack frame of current
	                 # acts as saving bp

    mov %rsp, (%rdi) # save sp of current stack
    mov (%rsi), %rsp # load sp of target stack

    pop (%rsi)       # load sp of previous stack frame of target
	                 # acts as restoring previous bp

	pop %rax         # align to 16 bytes

	call after_ctx_switch

	mov 0x28(%rsp), %r15
	mov 0x20(%rsp), %r14
	mov 0x18(%rsp), %r13
	mov 0x10(%rsp), %r12
	mov 0x08(%rsp), %rbp
    mov 0x00(%rsp), %rbx

	add $0x30, %rsp
    popf

    ret

.altmacro
.macro build_isr name
	.align 8
	ISR\name:
		call ISR_stub
.endm

.set i, 0
.rept 0x80+1
	build_isr %i
	.set i, i+1
.endr

.section .rodata

.align 8
.globl ISR_START_ADDR
.type  ISR_START_ADDR @object
ISR_START_ADDR:
	.quad ISR0
