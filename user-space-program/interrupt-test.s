.code32

.text

.globl main
main:
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

.data

__user_interrupt_test_string:
	.asciz "syscall 0x01 write: hello from user space\n"
