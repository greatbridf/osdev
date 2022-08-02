.code32

.text

.globl main
.type  main @function
main:
	pushl %ebp
	movl %esp, %ebp

	movl %esp, %eax

	movl %ebp, %esp
	popl %ebp

	jmp .
