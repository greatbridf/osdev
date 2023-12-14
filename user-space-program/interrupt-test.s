.code32

.text

.globl main
main:
# fork 1 -> 2
	movl $0x02, %eax
	int $0x80
	movl %eax, %esi
# fork 2 -> 4
	movl $0x02, %eax
	int $0x80
	movl %eax, %ecx
# write
	movl $0x04, %eax
	movl $1, %ebx
	movl $__user_interrupt_test_string, %ecx
	movl $(__user_interrupt_test_string_end - __user_interrupt_test_string), %edx
	int $0x80

	xorl %eax, %eax
	ret

.data

__user_interrupt_test_string:
	.ascii "syscall 0x01 write: hello from user space\n"
__user_interrupt_test_string_end:
