sign_bit:
	.quad 0x8000000000000000
one_dbl:
	.double 1.0
.L0:
	.asciz "FN()\n"
.L1:
	.asciz "%s i=%d N=%d\n"
.L2:
	.asciz "Starting\n"
.L3:
	.asciz "hello!"
.globl argc
	.comm argc, 8, 8
.globl argv
	.comm argv, 8, 8
	.text
	.global _FN
_FN:
	push   %rbp
	movq   %rsp, %rbp
	# printf(s: 0, i:16 f:0) sp: 0
	leaq   .L0(%rip), %rax
	push   %rax
	# LOAD LEAQ START: AST_TYPE_ARRAY
	leaq   0(%rbp), %rax
	# LOAD LEAQ END: AST_TYPE_ARRAY
	push   %rax
	pop    %rsi
	pop    %rdi
	call   _printf
	leave
	ret
.text
	.global _PrintMessage
_PrintMessage:
	push   %rbp
	movq   %rsp, %rbp
	subq   $32, %rsp #STACK LOCAL COUNT 0
	movq   %rdi, -32(%rbp)
	movq   %rsi, -24(%rbp)
	movq   %rdx, -16(%rbp)
	movq   %rcx, -8(%rbp)
	movq    -16(%rbp), %rax
	test   %rax, %rax
	je     .L4
	# fn(s: 0, i:0 f:0) sp: 32
	# LOAD rax AST_TYPE_FUNC START
	movq  -16(%rbp), %rax
	# LOAD rax END
	movq    %rax,%r11
	call    *%r11
.L4:
	# printf(s: 0, i:32 f:0) sp: 32
	leaq   .L1(%rip), %rax
	push   %rax
	# LOAD rax ->AST_TYPE_POINTER START
	movq  -32(%rbp), %rax
	# LOAD rax END
	push   %rax
	# LOAD rax AST_TYPE_INT START
	movq  -24(%rbp), %rax
	# LOAD rax END
	push   %rax
	# LOAD rax AST_TYPE_INT START
	movq  -8(%rbp), %rax
	# LOAD rax END
	push   %rax
	pop    %rcx
	pop    %rdx
	pop    %rsi
	pop    %rdi
	call   _printf
	leave
	ret
.text
	.global _main
_main:
	push   %rbp
	movq   %rsp, %rbp
	# printf(s: 0, i:16 f:0) sp: 32
	leaq   .L2(%rip), %rax
	push   %rax
	# LOAD LEAQ START: AST_TYPE_ARRAY
	leaq   0(%rbp), %rax
	# LOAD LEAQ END: AST_TYPE_ARRAY
	push   %rax
	pop    %rsi
	pop    %rdi
	call   _printf
	# PrintMessage(s: 0, i:32 f:0) sp: 32
	leaq   .L3(%rip), %rax
	push   %rax
	movq   $24, %rax
	push   %rax
	movq   $0, %rax
	push   %rax
	movq   $4, %rax
	push   %rax
	pop    %rcx
	pop    %rdx
	pop    %rsi
	pop    %rdi
	call   _PrintMessage
	leave
	ret
.LFE0:
	.ident      "hcc: apple aarch64 beta-v0.0.10"
