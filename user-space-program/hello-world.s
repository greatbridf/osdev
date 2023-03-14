.code32

.text

.globl main
main:
	movl $0xcbcbcbcb, %eax
	movl $0xacacacac, %edx

	movl $0x01, %eax
	movl $0, %edi
	movl $_str, %esi
	movl $_str_size, %ecx
	movl (%ecx), %edx
	int $0x80

	xorl %eax, %eax
	ret

.data
_str:
	.ascii "Hello, World!\n"

_str_size:
	.long (_str_size - _str)
