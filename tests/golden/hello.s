.section .text
main:
.type main, @function
	push %rbp
	mov %rsp, %rbp
	lea .Lstr0(%rip), %rax
	push %rax
	pop %rdi
	call flint_printstr
	movq $0, %rax
	push %rax
	pop %rax
	lea .Lstr1(%rip), %rax
	push %rax
	pop %rdi
	call flint_printstr
	movq $0, %rax
	push %rax
	pop %rax
	movq $0, %rax
	push %rax
	pop %rax
	leave
	ret
	xor %rax, %rax
	leave
	ret
.size main, .-main
.globl _start
_start:
	call flint_ignore_sigpipe
	call main
	mov %eax, %edi
	call flint_exit
.section .rodata
.Lstr0:	.asciz "hello, world"
.Lstr1:	.asciz "\n"