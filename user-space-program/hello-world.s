.code32

.text

.globl main
main:
	movl $0xcbcbcbcb, %eax
	movl $0xacacacac, %edx

	movl $0x04, %eax
	movl $1, %ebx
	movl $_str, %ecx
	movl $_str_size, %edx
	movl (%edx), %edx
	int $0x80

	xorl %eax, %eax
	ret

.data
_str:
	.ascii "Hello, World!\n"

_str_size:
	.long (_str_size - _str)
