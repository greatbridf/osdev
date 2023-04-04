.code32

.text

.globl main
main:
# fork 1 -> 2
	movl $57, %eax
	int $0x80
	movl %eax, %esi
# fork 2 -> 4
	movl $57, %eax
	int $0x80
	movl %eax, %ecx
# write
	movl $1, %eax
	movl $1, %edi
	movl $__user_interrupt_test_string, %esi
	movl $(__user_interrupt_test_string_end - __user_interrupt_test_string), %edx
	int $0x80

	xorl %eax, %eax
	ret

.data

__user_interrupt_test_string:
	.ascii "syscall 0x01 write: hello from user space\n"
__user_interrupt_test_string_end:
