.section .text
fib:
.type fib, @function
	push %rbp
	mov %rsp, %rbp
	sub $16, %rsp
	mov %rdi, -8(%rbp)
	movq -8(%rbp), %rax
	push %rax
	movq $2, %rax
	push %rax
	pop %rsi
	pop %rdi
	cmp %rsi, %rdi
	setl %al
	movzbl %al, %edi
	push %rdi
	pop %rax
	test %rax, %rax
	jz .Lif_else0
	movq -8(%rbp), %rax
	push %rax
	pop %rax
	add $16, %rsp
	leave
	ret
.Lif_else0:
.Lif_end0:
	movq -8(%rbp), %rax
	push %rax
	movq $1, %rax
	push %rax
	pop %rsi
	pop %rdi
	sub %rsi, %rdi
	push %rdi
	pop %rdi
	call fib
	push %rax
	movq -8(%rbp), %rax
	push %rax
	movq $2, %rax
	push %rax
	pop %rsi
	pop %rdi
	sub %rsi, %rdi
	push %rdi
	pop %rdi
	call fib
	push %rax
	pop %rsi
	pop %rdi
	add %rsi, %rdi
	push %rdi
	pop %rax
	add $16, %rsp
	leave
	ret
	xor %rax, %rax
	add $16, %rsp
	leave
	ret
.size fib, .-fib
main:
.type main, @function
	push %rbp
	mov %rsp, %rbp
	movq $10, %rax
	push %rax
	pop %rdi
	call fib
	push %rax
	pop %rdi
	call flint_printi64
	movq $0, %rax
	push %rax
	pop %rax
	lea .Lstr2(%rip), %rax
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
.Lstr2:	.asciz "\n"