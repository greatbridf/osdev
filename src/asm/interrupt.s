.text

#define RAX     0x00
#define RBX     0x08
#define RCX     0x10
#define RDX     0x18
#define RDI     0x20
#define RSI     0x28
#define R8      0x30
#define R9      0x38
#define R10     0x40
#define R11     0x48
#define R12     0x50
#define R13     0x58
#define R14     0x60
#define R15     0x68
#define RBP     0x70
#define INT_NO  0x78
#define ERRCODE 0x80
#define RIP     0x88
#define CS      0x90
#define FLAGS   0x98
#define RSP     0xa0
#define SS      0xa8

.macro movcfi reg, offset
	mov \reg, \offset(%rsp)
	.cfi_rel_offset \reg, \offset
.endm

.macro movrst reg, offset
	mov \offset(%rsp), \reg
	.cfi_restore \reg
.endm

.extern after_ctx_switch
.globl ISR_stub_restore

ISR_stub:
	.cfi_startproc
	.cfi_signal_frame
	.cfi_def_cfa_offset 0x18
	.cfi_offset %rsp, 0x10

	sub $0x78, %rsp
	.cfi_def_cfa_offset 0x90

	movcfi %rax, RAX
	movcfi %rbx, RBX
	movcfi %rcx, RCX
	movcfi %rdx, RDX
	movcfi %rdi, RDI
	movcfi %rsi, RSI
	movcfi %r8,  R8
	movcfi %r9,  R9
	movcfi %r10, R10
	movcfi %r11, R11
	movcfi %r12, R12
	movcfi %r13, R13
	movcfi %r14, R14
	movcfi %r15, R15
	movcfi %rbp, RBP

	mov INT_NO(%rsp), %rax
	sub $ISR0, %rax
	shr $3, %rax
	mov %rax, INT_NO(%rsp)

	mov %rsp, %rbx
	.cfi_def_cfa_register %rbx

	and $~0xf, %rsp
	sub $512, %rsp
	fxsave (%rsp)

	mov %rbx, %rdi
	mov %rsp, %rsi
	call interrupt_handler

ISR_stub_restore:
	fxrstor (%rsp)
	mov %rbx, %rsp
	.cfi_def_cfa_register %rsp

	movrst %rax, RAX
	movrst %rbx, RBX
	movrst %rcx, RCX
	movrst %rdx, RDX
	movrst %rdi, RDI
	movrst %rsi, RSI
	movrst %r8,  R8
	movrst %r9,  R9
	movrst %r10, R10
	movrst %r11, R11
	movrst %r12, R12
	movrst %r13, R13
	movrst %r14, R14
	movrst %r15, R15
	movrst %rbp, RBP

	add $0x88, %rsp
	.cfi_def_cfa_offset 0x08

	iretq
	.cfi_endproc

# parameters
# #1: sp* current_task_sp
# #2: sp* target_task_sp
.globl asm_ctx_switch
.type  asm_ctx_switch @function
asm_ctx_switch:
	.cfi_startproc
    pushf
	.cfi_def_cfa_offset 0x10

	sub $0x38, %rsp  # extra 8 bytes to align to 16 bytes
	.cfi_def_cfa_offset 0x48

	movcfi %rbx, 0x08
	movcfi %rbp, 0x10
	movcfi %r12, 0x18
	movcfi %r13, 0x20
	movcfi %r14, 0x28
	movcfi %r15, 0x30

    push (%rdi) 	 # save sp of previous stack frame of current
	                 # acts as saving bp
	.cfi_def_cfa_offset 0x50

    mov %rsp, (%rdi) # save sp of current stack
    mov (%rsi), %rsp # load sp of target stack

    pop (%rsi)       # load sp of previous stack frame of target
	                 # acts as restoring previous bp
	.cfi_def_cfa_offset 0x48

	pop %rax         # align to 16 bytes
	.cfi_def_cfa_offset 0x40

	call after_ctx_switch

	mov 0x28(%rsp), %r15
	mov 0x20(%rsp), %r14
	mov 0x18(%rsp), %r13
	mov 0x10(%rsp), %r12
	mov 0x08(%rsp), %rbp
    mov 0x00(%rsp), %rbx

	add $0x30, %rsp
	.cfi_def_cfa_offset 0x10

    popf
	.cfi_def_cfa_offset 0x08

    ret
	.cfi_endproc

.altmacro
.macro build_isr_no_err name
	.align 8
	.globl ISR\name
	.type  ISR\name @function
	ISR\name:
		.cfi_startproc
		.cfi_signal_frame
		.cfi_def_cfa_offset 0x08
		.cfi_offset %rsp, 0x10

		.cfi_same_value %rax
		.cfi_same_value %rbx
		.cfi_same_value %rcx
		.cfi_same_value %rdx
		.cfi_same_value %rdi
		.cfi_same_value %rsi
		.cfi_same_value %r8
		.cfi_same_value %r9
		.cfi_same_value %r10
		.cfi_same_value %r11
		.cfi_same_value %r12
		.cfi_same_value %r13
		.cfi_same_value %r14
		.cfi_same_value %r15
		.cfi_same_value %rbp

		push %rbp # push placeholder for error code
		.cfi_def_cfa_offset 0x10

		call ISR_stub
		.cfi_endproc
.endm

.altmacro
.macro build_isr_err name
	.align 8
	.globl ISR\name
	.type  ISR\name @function
	ISR\name:
		.cfi_startproc
		.cfi_signal_frame
		.cfi_def_cfa_offset 0x10
		.cfi_offset %rsp, 0x10

		.cfi_same_value %rax
		.cfi_same_value %rbx
		.cfi_same_value %rcx
		.cfi_same_value %rdx
		.cfi_same_value %rdi
		.cfi_same_value %rsi
		.cfi_same_value %r8
		.cfi_same_value %r9
		.cfi_same_value %r10
		.cfi_same_value %r11
		.cfi_same_value %r12
		.cfi_same_value %r13
		.cfi_same_value %r14
		.cfi_same_value %r15
		.cfi_same_value %rbp

		call ISR_stub
		.cfi_endproc
.endm

build_isr_no_err 0
build_isr_no_err 1
build_isr_no_err 2
build_isr_no_err 3
build_isr_no_err 4
build_isr_no_err 5
build_isr_no_err 6
build_isr_no_err 7
build_isr_err    8
build_isr_no_err 9
build_isr_err    10
build_isr_err    11
build_isr_err    12
build_isr_err    13
build_isr_err    14
build_isr_no_err 15
build_isr_no_err 16
build_isr_err    17
build_isr_no_err 18
build_isr_no_err 19
build_isr_no_err 20
build_isr_err    21
build_isr_no_err 22
build_isr_no_err 23
build_isr_no_err 24
build_isr_no_err 25
build_isr_no_err 26
build_isr_no_err 27
build_isr_no_err 28
build_isr_err    29
build_isr_err    30
build_isr_no_err 31

.set i, 32
.rept 0x80+1
	build_isr_no_err %i
	.set i, i+1
.endr

.section .rodata

.align 8
.globl ISR_START_ADDR
.type  ISR_START_ADDR @object
ISR_START_ADDR:
	.quad ISR0
