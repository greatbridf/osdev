.code32

.text

.globl main
main:
	movl $0xcbcbcbcb, %eax
	movl $0xacacacac, %ebx
	jmp .
