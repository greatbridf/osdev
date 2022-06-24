.code32
.section .text.user-space

.globl user_hello_world
user_hello_world:
	movl $0xcbcbcbcb, %eax
	movl $0xacacacac, %ebx
	jmp .
