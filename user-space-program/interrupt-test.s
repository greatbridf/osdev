.code32
.section .text.user-space

.globl user_interrupt_test
user_interrupt_test:
# fork 1 -> 2
	xorl %eax, %eax
	int $0x80
	movl %eax, %esi
# fork 2 -> 4
	xorl %eax, %eax
	int $0x80
	movl %eax, %ecx
# write
	movl $1, %eax
	movl $__user_interrupt_test_string, %edi
	int $0x80
# sleep
	movl $2, %eax
	movl $0xffffffff, %edi
	int $0x80
# noreturn
	jmp .

__user_interrupt_test_string:
	.ascii "syscall 0x01 write: hello from user space\n\0"
