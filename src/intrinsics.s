# flint runtime: freestanding x86-64 Linux helpers (System V ABI)
# These are linked in with generated user code. No libc.

    .section .bss
    .lcomm __flint_heap_top, 8

    .section .text

    .globl flint_exit
    .type flint_exit, @function
flint_exit:
    mov $231, %rax            # SYS_exit_group
    syscall
    .size flint_exit, .-flint_exit

    .globl flint_write
    .type flint_write, @function
flint_write:
    mov $1, %rax              # SYS_write
    syscall
    ret
    .size flint_write, .-flint_write

    .globl flint_read
    .type flint_read, @function
flint_read:
    mov $0, %rax              # SYS_read
    syscall
    ret
    .size flint_read, .-flint_read

    .globl flint_open
    .type flint_open, @function
flint_open:
    mov $2, %rax              # SYS_open
    syscall
    ret
    .size flint_open, .-flint_open

    .globl flint_close
    .type flint_close, @function
flint_close:
    mov $3, %rax              # SYS_close
    syscall
    ret
    .size flint_close, .-flint_close

    .globl flint_brk
    .type flint_brk, @function
flint_brk:
    mov $12, %rax             # SYS_brk
    syscall
    ret
    .size flint_brk, .-flint_brk

    .globl flint_ignore_sigpipe
    .type flint_ignore_sigpipe, @function
# signal(SIGPIPE, SIG_IGN): a closed peer (e.g. the compositor disconnecting)
# then yields EPIPE on write instead of killing the process.
flint_ignore_sigpipe:
    mov $13, %rax             # SYS_signal
    mov $13, %edi             # SIGPIPE
    mov $1, %esi              # SIG_IGN
    syscall
    ret
    .size flint_ignore_sigpipe, .-flint_ignore_sigpipe

    .globl flint_socket
    .type flint_socket, @function
flint_socket:
    mov $41, %rax            # SYS_socket
    syscall
    ret
    .size flint_socket, .-flint_socket

    .globl flint_bind
    .type flint_bind, @function
flint_bind:
    mov $49, %rax            # SYS_bind
    syscall
    ret
    .size flint_bind, .-flint_bind

    .globl flint_listen
    .type flint_listen, @function
flint_listen:
    mov $50, %rax            # SYS_listen
    syscall
    ret
    .size flint_listen, .-flint_listen

    .globl flint_accept
    .type flint_accept, @function
flint_accept:
    mov $43, %rax            # SYS_accept
    syscall
    ret
    .size flint_accept, .-flint_accept

    .globl flint_connect
    .type flint_connect, @function
flint_connect:
    mov $42, %rax            # SYS_connect
    syscall
    ret
    .size flint_connect, .-flint_connect

    .globl flint_sockaddr
    .type flint_sockaddr, @function
# flint_sockaddr(port, o1, o2, o3, o4) -> *int
# Builds a 16-byte AF_INET sockaddr_in on the heap and returns it.
# args: rdi=port, rsi=o1, rdx=o2, rcx=o3, r8=o4
# layout: [family:2][port:2 BE][o1 o2 o3 o4][zero:8]
flint_sockaddr:
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # port
    mov %rsi, %r13            # o1
    mov %rdx, %r14            # o2
    mov %rcx, %r15            # o3
    mov %r8, %rbp             # o4
    mov $16, %rdi
    call flint_alloc            # rax = buf (clobbers rdi/rsi/rdx/rcx/r8/r9/r10/r11)
    mov %rax, %rbx            # buf
    movw $2, (%rbx)           # family = AF_INET (bytes 0-1)
    # port big-endian at bytes 2-3: swap the two bytes
    xor %r10, %r10           # zero r10 (movb does not zero-extend)
    mov %r12b, %r10b          # r10 = low byte
    shr $8, %r12             # r12 = high byte
    shl $8, %r10             # r10 = low byte << 8
    or %r12, %r10            # r10 = (low << 8) | high
    mov %r10w, 2(%rbx)
    mov %r13b, 4(%rbx)       # o1
    mov %r14b, 5(%rbx)       # o2
    mov %r15b, 6(%rbx)       # o3
    mov %bpl, 7(%rbx)        # o4
    # bytes 8-15 are already 0 (mmap zero-fills)
    mov %rbx, %rax
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret
    .size flint_sockaddr, .-flint_sockaddr

    .globl flint_alloc
    .type flint_alloc, @function
flint_alloc:
    mov %rdi, %r10            # n
    lea 15(%r10), %r10
    and $-16, %r10            # align size up to 16 (min 16)
    # mmap(NULL, size, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0)
    mov %r10, %rsi            # rsi = length
    xor %edi, %edi            # rdi = addr (NULL)
    mov $3, %rdx              # rdx = prot (READ|WRITE)
    mov $0x22, %r10           # r10 = flags (MAP_PRIVATE|MAP_ANONYMOUS)
    mov $-1, %r8              # r8  = fd (-1)
    xor %r9, %r9              # r9  = offset (0)
    mov $9, %rax              # rax = SYS_mmap
    syscall                   # rax = pointer (or -errno on failure)
    ret
    .size flint_alloc, .-flint_alloc

    .globl flint_free
    .type flint_free, @function
flint_free:
    ret                        # bump allocator: free is a no-op in v1
    .size flint_free, .-flint_free

    .globl flint_retain
    .type flint_retain, @function
flint_retain:
    test %rdi, %rdi
    jz .Lretain_done
    incq (%rdi)
.Lretain_done:
    ret
    .size flint_retain, .-flint_retain

    .globl flint_retain_val
    .type flint_retain_val, @function
# flint_retain_val(v): null-guarded retain; returns v in rax (so the value
# stays usable in a typed slot of any class).
flint_retain_val:
    mov %rdi, %rax
    test %rdi, %rdi
    jz .Lretain_val_done
    incq (%rdi)
.Lretain_val_done:
    ret
    .size flint_retain_val, .-flint_retain_val

    .globl flint_fn_call2
    .type flint_fn_call2, @function
# flint_fn_call2(block, a): invoke a closure. `block` is [fn_ptr][cap...];
# the lifted function is called as fn(ctx = block, a).
# args: rdi=block, rsi=a
flint_fn_call2:
    # rdi = closure block (ctx), rsi = the lambda's single argument;
    # the lifted fn expects exactly (rdi=ctx, rsi=arg).
    call *0(%rdi)
    ret
    .size flint_fn_call2, .-flint_fn_call2

    .globl flint_release
    .type flint_release, @function
flint_release:
    decq (%rdi)
    jnz .Lrelease_done
    # refcount zero: free the object (n+1)*8 via munmap? For v1, leak (no-op) is safe; just return.
    # Could call flint_free or munmap here, but we keep it simple.
.Lrelease_done:
    ret
    .size flint_release, .-flint_release

    .globl flint_memcpy
    .type flint_memcpy, @function
# flint_memcpy(dst, src, n): word loop while n >= 8, then a byte tail.
# Leaves %rdi = dst + n. n need not be a multiple of 8.
flint_memcpy:
    test %rdx, %rdx
    jz 1f
    mov %rdx, %r10
.Lmc_w:
    cmp $8, %r10
    jb .Lmc_b
    movq (%rsi), %rax
    movq %rax, (%rdi)
    lea 8(%rdi), %rdi
    lea 8(%rsi), %rsi
    sub $8, %r10
    jmp .Lmc_w
.Lmc_b:
    test %r10, %r10
    jz 1f
    movzbl (%rsi), %eax
    movb %al, (%rdi)
    inc %rdi
    inc %rsi
    dec %r10
    jmp .Lmc_b
1:
    ret
    .size flint_memcpy, .-flint_memcpy

    .globl flint_strlen
    .type flint_strlen, @function
flint_strlen:
    xor %rax, %rax
.Lsl_loop:
    movzbl (%rdi, %rax), %ecx
    test %ecx, %ecx
    jz .Lsl_done
    inc %rax
    jmp .Lsl_loop
.Lsl_done:
    ret
    .size flint_strlen, .-flint_strlen

    .globl flint_printstr
    .type flint_printstr, @function
flint_printstr:
    mov %rdi, %rsi            # buf
    call flint_strlen
    mov %rax, %rdx            # len
    mov $1, %rdi              # stdout
    mov $1, %rax              # SYS_write
    syscall
    ret
    .size flint_printstr, .-flint_printstr

    .globl flint_atoi
    .type flint_atoi, @function
flint_atoi:
    mov %rdi, %rax
    xor %rdx, %rdx            # result
    xor %rcx, %rcx            # negative flag
.atoi_skip:
    movzbl (%rax), %r8d
    test %r8d, %r8d
    jz .atoi_finish
    cmp $' ', %r8d
    jg .atoi_start
    inc %rax
    jmp .atoi_skip
.atoi_start:
    cmp $'-', %r8d
    je .atoi_neg
    jmp .atoi_loop
.atoi_neg:
    or $1, %rcx
    inc %rax
.atoi_loop:
    movzbl (%rax), %r8d
    test %r8d, %r8d
    jz .atoi_finish
    cmp $'0', %r8d
    jl .atoi_finish
    cmp $'9', %r8d
    jg .atoi_finish
    mov %rdx, %r9
    lea (%rdx,%rdx,4), %r9    # r9 = rdx*5
    lea (%r9,%r9), %r9        # r9 = rdx*10
    sub $'0', %r8
    add %r8, %r9
    mov %r9, %rdx
    inc %rax
    jmp .atoi_loop
.atoi_finish:
    mov %rdx, %rax
    test %rcx, %rcx
    jz .atoi_done
    neg %rax
.atoi_done:
    ret
    .size flint_atoi, .-flint_atoi

    .globl flint_len
    .type flint_len, @function
flint_len:
    movq (%rdi), %rax         # slot 0 = array length header
    ret
    .size flint_len, .-flint_len

    .globl flint_strcmp
    .type flint_strcmp, @function
flint_strcmp:
    xor %rax, %rax            # i
.Lsc_loop:
    movzbl (%rdi, %rax), %ecx # a[i]
    movzbl (%rsi, %rax), %edx # b[i]
    cmp %edx, %ecx
    jne .Lsc_done
    test %ecx, %ecx
    jz .Lsc_eq
    inc %rax
    jmp .Lsc_loop
.Lsc_eq:
    xor %rax, %rax
    ret
.Lsc_done:
    sub %edx, %ecx
    mov %ecx, %eax            # rax = zero-extended diff
    test %ecx, %ecx
    jns 1f                    # non-negative: rax already correct
    or $-1, %rax              # negative: sign-extend
1:
    ret
    .size flint_strcmp, .-flint_strcmp

    .globl flint_strcopy
    .type flint_strcopy, @function
flint_strcopy:
    # flint_alloc clobbers r10 and (via syscall) r11/rcx, so keep src and
    # size in callee-saved registers
    push %rbx
    push %r12
    mov %rdi, %rbx            # src
    call flint_strlen           # rax = len
    lea 1(%rax), %r12         # n = len+1
    mov %r12, %rdi            # size
    call flint_alloc            # rax = dst
    mov %rbx, %rsi            # src
    mov %rax, %rdi            # dst
    mov %r12, %rdx            # n
    call flint_memcpy           # rdi = dst + n
    sub %r12, %rdi            # dst
    mov %rdi, %rax
    pop %r12
    pop %rbx
    ret
    .size flint_strcopy, .-flint_strcopy

    .globl flint_strconcat
    .type flint_strconcat, @function
# flint_strconcat(a, b): dst = alloc(la+lb+1); copy a then b. The MAP_ANONYMOUS
# buffer is zero-filled, so the trailing NUL needs no write. flint_memcpy leaves
# %rdi = dst + n, so the second copy starts where the first ended.
flint_strconcat:
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # a
    mov %rsi, %r13            # b
    mov %r12, %rdi
    call flint_strlen           # rax = la
    mov %rax, %r14            # la
    mov %r13, %rdi
    call flint_strlen           # rax = lb
    mov %rax, %r15            # lb
    lea 1(%r14, %r15), %rdi   # size
    call flint_alloc            # rax = dst
    mov %r12, %rsi            # a
    mov %rax, %rdi            # dst
    mov %r14, %rdx            # la
    call flint_memcpy           # rdi = dst + la
    mov %r13, %rsi            # b
    mov %r15, %rdx            # lb
    call flint_memcpy           # rdi = dst + la + lb
    sub %r15, %rdi
    sub %r14, %rdi
    mov %rdi, %rax            # dst
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    ret
    .size flint_strconcat, .-flint_strconcat

    .globl flint_printi64
    .type flint_printi64, @function
flint_printi64:
    push %rbp
    mov %rsp, %rbp
    sub $48, %rsp
    lea -32(%rbp), %rsi       # buf base
    mov %rdi, %r10            # value
    mov %rbp, %r11            # p = base+32
    test %r10, %r10
    jns .Lpi_pos
    neg %r10
    mov $1, %rcx              # negative flag
    jmp .Lpi_sign
.Lpi_pos:
    xor %rcx, %rcx
.Lpi_sign:
    test %r10, %r10
    jnz .Lpi_loop
    dec %r11
    movb $48, (%r11)          # '0'
    jmp .Lpi_negcheck
.Lpi_loop:
    mov $10, %r9
    mov %r10, %rax
.Lpi_div:
    xor %rdx, %rdx
    div %r9
    lea 48(%rdx), %rdx
    dec %r11
    movb %dl, (%r11)
    mov %rax, %r10
    test %r10, %r10
    jnz .Lpi_div
.Lpi_negcheck:
    test %rcx, %rcx
    jz .Lpi_done
    dec %r11
    movb $45, (%r11)          # '-' before digits
.Lpi_done:
    mov %rbp, %rdx
    sub %r11, %rdx            # len
    mov %r11, %rsi            # first byte
    mov $1, %rdi              # stdout
    mov $1, %rax
    syscall
    leave
    ret
    .size flint_printi64, .-flint_printi64

    .globl flint_println_str
    .type flint_println_str, @function
# flint_println_str(s): rdi = s. Prints s then a newline to stdout.
flint_println_str:
    call flint_printstr
    lea .Lprint_nl(%rip), %rsi
    mov $1, %rdi              # fd = 1 (stdout)
    mov $1, %rdx             # len = 1
    mov $1, %rax             # SYS_write
    syscall
    ret
    .size flint_println_str, .-flint_println_str

    .globl flint_println_i64
    .type flint_println_i64, @function
# flint_println_i64(n): rdi = n. Prints n then a newline to stdout.
flint_println_i64:
    call flint_printi64
    lea .Lprint_nl(%rip), %rsi
    mov $1, %rdi              # fd = 1 (stdout)
    mov $1, %rdx             # len = 1
    mov $1, %rax             # SYS_write
    syscall
    ret
    .size flint_println_i64, .-flint_println_i64

    .globl flint_itoa
    .type flint_itoa, @function
# flint_itoa(n): buf = alloc(digits(n) + 1); write itoa(n) into buf; return buf.
# The MAP_ANONYMOUS buffer is zero-filled, so the trailing NUL needs no write.
# flint_alloc clobbers rdi/rsi/rdx/rcx/r8/r9/r10/r11; keep digits_len in r14.
flint_itoa:
    push %rbp
    mov %rsp, %rbp
    sub $48, %rsp
    push %r12
    push %r13
    push %r14
    push %r11
    push %r10
    push %r9
    lea -32(%rbp), %rsi       # buf base
    mov %rdi, %r12            # r12 = n
    mov %rbp, %r13            # r13 = p = base+32
    mov %r12, %r10            # r10 = value
    test %r10, %r10
    jns .Litoa_pos
    neg %r10
    mov $1, %r14             # r14 = negative flag
    jmp .Litoa_sign
.Litoa_pos:
    xor %r14, %r14
.Litoa_sign:
    test %r10, %r10
    jnz .Litoa_loop
    dec %r13
    movb $48, (%r13)          # '0'
    jmp .Litoa_negcheck
.Litoa_loop:
    mov $10, %r9
    mov %r10, %rax
.Litoa_div:
    xor %rdx, %rdx
    div %r9
    lea 48(%rdx), %rdx
    dec %r13
    movb %dl, (%r13)
    mov %rax, %r10
    test %r10, %r10
    jnz .Litoa_div
.Litoa_negcheck:
    test %r14, %r14
    jz .Litoa_done
    dec %r13
    movb $45, (%r13)          # '-' before digits
.Litoa_done:
    # r13 = first byte, rbp = base+32. digits_len = rbp - r13
    mov %rbp, %r14
    sub %r13, %r14            # r14 = digits_len
    lea 1(%r14), %rdi        # size = digits_len + 1
    call flint_alloc            # rax = dst (clobbers r13, r10, rdx, ...)
    # flint_alloc clobbered r13 (first byte). Recompute: r13 = rbp - r14.
    mov %rbp, %r13
    sub %r14, %r13            # r13 = first byte (recomputed)
    # copy digits into dst
    mov %r13, %rsi            # first byte
    mov %rax, %rdi            # dst
    mov %r14, %rdx            # digits_len
    call flint_memcpy           # rdi = dst + digits_len
    xor %al, %al
    mov %al, (%rdi)          # NUL terminate
    sub %r14, %rdi
    mov %rdi, %rax            # dst
    pop %r9
    pop %r10
    pop %r11
    pop %r14
    pop %r13
    pop %r12
    leave
    ret
    .size flint_itoa, .-flint_itoa

    .globl flint_time_millis
    .type flint_time_millis, @function
# flint_time_millis(): returns the current wall-clock time in milliseconds.
# Uses gettimeofday(&tv, NULL); result = tv.tv_sec*1000 + tv.tv_usec/1000.
# (clock_gettime is blocked in the sandbox; gettimeofday works.)
# gettimeofday (96) clobbers rcx and r11; keep the parts in r10/r12.
flint_time_millis:
    push %rbp
    mov %rsp, %rbp
    sub $32, %rsp
    lea -16(%rbp), %rdi       # &tv (8 bytes: tv_sec + tv_usec)
    xor %rsi, %rsi           # tz = NULL
    mov $96, %rax            # SYS_gettimeofday
    syscall
    movq -16(%rbp), %r10       # r10 = tv_sec
    mov $1000, %rax
    imul %rax, %r10            # r10 = tv_sec * 1000
    movq -8(%rbp), %rax        # rax = tv_usec
    xor %rdx, %rdx           # rdx = 0 (high part for div)
    mov $1000, %r12        # r12 = divisor (usec -> ms)
    div %r12                 # rax = tv_usec / 1000 (quotient)
    add %rax, %r10           # r10 = tv_sec*1000 + tv_usec/1000
    mov %r10, %rax           # return in rax
    leave
    ret
    .size flint_time_millis, .-flint_time_millis

    .globl flint_rand_next
    .type flint_rand_next, @function
# flint_rand_next(): returns a 64-bit random value from getrandom (318).
flint_rand_next:
    push %rbp
    mov %rsp, %rbp
    sub $16, %rsp
    lea -8(%rbp), %rdi        # &buf
    mov $8, %rsi              # len = 8
    xor %rdx, %rdx           # flags = 0
    mov $318, %rax            # SYS_getrandom
    syscall
    movq -8(%rbp), %rax        # return the random value
    leave
    ret
    .size flint_rand_next, .-flint_rand_next

    .globl flint_rand_range
    .type flint_rand_range, @function
# flint_rand_range(min, max): rdi = min, rsi = max. Returns min + (rand % (max - min)).
# getrandom (318) clobbers rcx and r11; keep min/max in r10/r12.
flint_rand_range:
    push %rbp
    mov %rsp, %rbp
    sub $16, %rsp
    push %r12
    mov %rdi, %r10            # r10 = min
    mov %rsi, %r12            # r12 = max
    lea -8(%rbp), %rdi        # &buf
    mov $8, %rsi              # len = 8
    xor %rdx, %rdx           # flags = 0
    mov $318, %rax            # SYS_getrandom
    syscall
    movq -8(%rbp), %rax        # rax = random
    test %rax, %rax
    jns .Lrr_abs
    neg %rax
.Lrr_abs:
    sub %r10, %r12           # r12 = max - min
    xor %rdx, %rdx           # rdx = 0 (high part for div)
    div %r12                 # rax = quotient, rdx = rand % (max - min)
    mov %rdx, %rax           # rax = remainder
    add %r10, %rax           # rax = min + (rand % (max - min))
    pop %r12
    leave
    ret
    .size flint_rand_range, .-flint_rand_range

    .globl flint_env_get
    .type flint_env_get, @function
# flint_env_get(name): rdi = name. Returns a heap-allocated copy of the value for
# "name" in /proc/self/environ, or 0 if not found. The environ is a sequence of
# NUL-separated "KEY=VALUE" strings.
flint_env_get:
    push %rbp
    mov %rsp, %rbp
    sub $4096, %rsp            # buffer for the environ
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = name
    # --- open /proc/self/environ ---
    lea .Lenv_path(%rip), %rdi
    xor %rsi, %rsi            # flags = O_RDONLY (0)
    xor %rdx, %rdx
    mov $2, %rax              # SYS_open
    syscall
    test %rax, %rax
    js .Lenv_done
    mov %rax, %r13            # r13 = fd
    # --- read the environ ---
    lea -4088(%rbp), %rsi     # buf
    mov $4088, %rdx           # len
    mov %r13, %rdi           # fd
    mov $0, %rax             # SYS_read
    syscall
    mov %rax, %r15            # r15 = bytes read
    # --- close the fd ---
    mov %r13, %rdi
    mov $3, %rax             # SYS_close
    syscall
    test %r15, %r15
    jz .Lenv_done
    # --- name_len ---
    mov %r12, %rdi
    call flint_strlen           # rax = name_len
    mov %rax, %r13            # r13 = name_len
    # --- scan the buffer byte by byte for "name=" ---
    lea -4088(%rbp), %rbx     # rbx = i (current position)
    lea -4088(%rbp), %r14     # r14 = end
    add %r15, %r14            # r14 = buf + bytes_read
 .Lenv_scan:
    cmp %rbx, %r14
    jbe .Lenv_done
    # --- does "name=" start at rbx? (compare name_len bytes, then '=') ---
    mov %r12, %r10           # r10 = name
    mov %rbx, %r11           # r11 = i
    mov %r13, %r8            # r8 = count
 .Lenv_match:
    test %r8, %r8
    jz .Lenv_match_ok
    movzbl (%r10), %ecx
    movzbl (%r11), %edx
    cmp %dl, %cl
    jne .Lenv_next
    inc %r10
    inc %r11
    dec %r8
    jmp .Lenv_match
 .Lenv_match_ok:
    # name matched; the byte at i + name_len must be '='
    mov %rbx, %r11
    add %r13, %r11           # r11 = i + name_len
    movzbl (%r11), %eax
    cmp $61, %al             # '=' is 61
    jne .Lenv_next
    # --- found: value = i + name_len + 1 ---
    inc %r11                # r11 = value start
    mov %r11, %r14          # r14 = value (preserve)
    mov %r11, %rdi
    call flint_strlen           # rax = value_len
    mov %rax, %r13          # r13 = value_len (preserve)
    lea 1(%r13), %rdi        # size = value_len + 1
    call flint_alloc            # rax = dst
    mov %rax, %r12          # r12 = dst (flint_memcpy clobbers rax)
    mov %r14, %rsi           # src = value
    mov %r12, %rdi           # dst
    mov %r13, %rdx           # n = value_len
    call flint_memcpy           # copies value_len bytes
    mov %r12, %rax           # rax = dst
    jmp .Lenv_ret
 .Lenv_next:
    inc %rbx                # advance to the next byte
    jmp .Lenv_scan
 .Lenv_done:
    xor %rax, %rax           # return 0 (not found)
 .Lenv_ret:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_env_get, .-flint_env_get
    .section .rodata
    .Lenv_path:
    .string "/proc/self/environ"
    .section .text

    .globl flint_strconcati
    .type flint_strconcati, @function
# flint_strconcati(s, n): dst = alloc(strlen(s) + digits(n) + 1); copy s then itoa(n).
# The MAP_ANONYMOUS buffer is zero-filled, so the trailing NUL needs no write.
# flint_alloc (mmap) clobbers caller arg regs and the stack, so the itoa is written
# directly into the allocated buffer AFTER flint_alloc (no stack buffer is reused).
flint_strconcati:
    push %rbp
    mov %rsp, %rbp
    sub $48, %rsp
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = s
    mov %rsi, %r13            # r13 = n
    # --- count digits in n (no buffer writes) ---
    mov %r13, %r10            # r10 = n
    xor %r15, %r15            # r15 = digit count
    test %r10, %r10
    jns .Lsci_pos
    neg %r10
    mov $1, %r8              # r8 = negative flag
    jmp .Lsci_count
.Lsci_pos:
    xor %r8, %r8
.Lsci_count:
    test %r10, %r10
    jz .Lsci_zero
.Lsci_count_loop:
    mov $10, %r9
    mov %r10, %rax
    xor %rdx, %rdx
    div %r9
    inc %r15
    mov %rax, %r10
    test %r10, %r10
    jnz .Lsci_count_loop
    jmp .Lsci_done_count
.Lsci_zero:
    mov $1, %r15            # "0" has 1 digit
.Lsci_done_count:
    test %r8, %r8
    jz .Lsci_no_neg
    inc %r15
.Lsci_no_neg:
    # --- len(s) ---
    mov %r12, %rdi            # s
    call flint_strlen           # rax = len(s)
    mov %rax, %r14            # r14 = len(s)
    # --- allocate ---
    lea 1(%r14, %r15), %rdi   # size = len(s) + digits + 1
    call flint_alloc            # rax = dst (clobbers rdi/rsi/rdx/rcx/r8/r9/r10/r11)
    # flint_alloc clobbered r8 (negative flag). Re-derive the sign.
    mov %r13, %r10            # r10 = n
    test %r10, %r10
    jns .Lsci_pos2
    neg %r10
    mov $1, %r8              # r8 = negative flag
    jmp .Lsci_sign2
.Lsci_pos2:
    xor %r8, %r8
.Lsci_sign2:
    # --- copy s into dst ---
    mov %r12, %rsi            # s
    mov %rax, %rdi            # dst
    mov %r14, %rdx            # len(s)
    call flint_memcpy           # rdi = dst + len(s)
    # flint_memcpy clobbered r10; re-derive |n| (r8 is the negative flag, preserved)
    mov %r13, %r10            # r10 = n
    test %r8, %r8
    jz .Lsci_abs
    neg %r10
.Lsci_abs:
    # --- itoa into dst + len(s) (one past the end = dst + len(s) + digits) ---
    # rdi = dst + len(s), so r11 = rdi + digits.
    lea (%rdi, %r15), %r11    # r11 = dst + len(s) + digits (one past the end)
    test %r10, %r10
    jnz .Lsci_loop2
    dec %r11
    movb $48, (%r11)          # '0'
    jmp .Lsci_negcheck2
.Lsci_loop2:
    mov $10, %r9
    mov %r10, %rax
.Lsci_div2:
    xor %rdx, %rdx
    div %r9
    lea 48(%rdx), %rdx
    dec %r11
    movb %dl, (%r11)
    mov %rax, %r10
    test %r10, %r10
    jnz .Lsci_div2
.Lsci_negcheck2:
    test %r8, %r8
    jz .Lsci_done2
    dec %r11
    movb $45, (%r11)          # '-' before digits
.Lsci_done2:
    # rdi = dst + len(s) (flint_memcpy leaves it there); recover dst
    sub %r14, %rdi
    mov %rdi, %rax            # dst
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    ret
    .size flint_strconcati, .-flint_strconcati

    .globl flint_str_substring
    .type flint_str_substring, @function
# flint_str_substring(s, start, len): rdi=s, rsi=start, rdx=len.
# Returns a heap-allocated copy of s[start..start+len] with NUL terminator.
flint_str_substring:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %r12            # r12 = s
    mov %rsi, %r13            # r13 = start
    mov %rdx, %r14            # r14 = len
    lea 1(%r14), %rdi        # size = len + 1
    call flint_alloc            # rax = dst (clobbers rdi/rsi/rdx/rcx/r8/r9/r10/r11)
    mov %rax, %rbx            # rbx = dst
    lea (%r12, %r13), %rsi   # src = s + start
    mov %rbx, %rdi           # dst
    mov %r14, %rdx           # n = len
    call flint_memcpy           # copies len bytes
    mov %rbx, %rax           # return dst
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_str_substring, .-flint_str_substring

    .globl flint_str_indexof
    .type flint_str_indexof, @function
# flint_str_indexof(s, sub): rdi=s, rsi=sub.
# Returns the index of the first occurrence of sub in s, or -1 if not found.
flint_str_indexof:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = s
    mov %rsi, %r13            # r13 = sub
    mov %r13, %rdi
    call flint_strlen           # rax = sub_len
    mov %rax, %r14           # r14 = sub_len
    test %r14, %r14
    jz .Lsio_empty           # empty sub: return 0
    mov %r12, %rdi
    call flint_strlen           # rax = s_len
    mov %rax, %r15           # r15 = s_len
    # max_start = s_len - sub_len
    sub %r14, %r15           # r15 = s_len - sub_len
    xor %rbx, %rbx           # rbx = i (current position)
.Lsio_scan:
    cmp %rbx, %r15
    jb .Lsio_notfound
    mov %r12, %r10           # r10 = s + i
    add %rbx, %r10
    mov %r13, %r11           # r11 = sub
    mov %r14, %r8            # r8 = count
.Lsio_match:
    test %r8, %r8
    jz .Lsio_found
    movzbl (%r10), %ecx
    movzbl (%r11), %edx
    cmp %dl, %cl
    jne .Lsio_next
    inc %r10
    inc %r11
    dec %r8
    jmp .Lsio_match
.Lsio_found:
    mov %rbx, %rax           # return i
    jmp .Lsio_ret
.Lsio_next:
    inc %rbx
    jmp .Lsio_scan
.Lsio_notfound:
    mov $-1, %rax            # return -1
    jmp .Lsio_ret
.Lsio_empty:
    mov $0, %rax
.Lsio_ret:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_str_indexof, .-flint_str_indexof

    .globl flint_str_replace
    .type flint_str_replace, @function
# flint_str_replace(s, old, new): rdi=s, rsi=old, rdx=new.
# Returns a new string with all occurrences of old replaced by new.
flint_str_replace:
    push %rbp
    mov %rsp, %rbp
    sub $64, %rsp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = s
    mov %rsi, %r13            # r13 = old
    mov %rdx, %r14            # r14 = new
    mov %rdx, -24(%rbp)      # [rbp-24] = new (saved)
    # --- lengths (save to stack before any call clobbers) ---
    mov %r12, %rdi
    call flint_strlen           # rax = s_len
    mov %rax, %r15           # r15 = s_len
    mov %r13, %rdi
    call flint_strlen           # rax = old_len
    mov %rax, %rbx           # rbx = old_len
    test %rbx, %rbx
    jz .Lsr_empty_old        # empty old: return a copy of s
    mov %r14, %rdi
    call flint_strlen           # rax = new_len
    mov %rax, -8(%rbp)       # [rbp-8] = new_len
    # --- count occurrences of old in s ---
    mov %r15, %r10           # r10 = s_len
    sub %rbx, %r10           # r10 = s_len - old_len (max start)
    xor %r9, %r9             # r9 = count
    xor %r11, %r11           # r11 = i
.Lsr_count:
    cmp %r11, %r10
    jb .Lsr_done_count
    mov %r12, %r8            # r8 = s + i
    add %r11, %r8
    mov %r13, %rax           # rax = old
    mov %rbx, %r14          # r14 = byte count (temp)
.Lsr_cmatch:
    test %r14, %r14
    jz .Lsr_cfound
    movzbl (%rax), %ecx
    movzbl (%r8), %edx
    cmp %dl, %cl
    jne .Lsr_cnext
    inc %rax
    inc %r8
    dec %r14
    jmp .Lsr_cmatch
.Lsr_cfound:
    inc %r9                # count++
    add %rbx, %r11        # i += old_len
    jmp .Lsr_count
.Lsr_cnext:
    inc %r11
    jmp .Lsr_count
.Lsr_done_count:
    # --- new total length = s_len + count * (new_len - old_len) ---
    mov -8(%rbp), %r10       # r10 = new_len
    sub %rbx, %r10           # r10 = delta = new_len - old_len
    mov %r9, %rax            # rax = count
    imul %r10, %rax          # rax = count * delta
    add %r15, %rax           # rax = s_len + count * delta
    # --- allocate ---
    lea 1(%rax), %rdi        # size = total + 1
    call flint_alloc            # rax = dst
    mov %rax, %rbx           # rbx = dst
    # --- build result ---
    mov -24(%rbp), %r14       # restore r14 = new
    # Stack: [rbp-8] = new_len, r12=s, r13=old, r14=new, r15=s_len, rbx=dst
    # We need old_len for the build loop. Save it.
    mov %r13, %rdi
    call flint_strlen           # rax = old_len
    mov %rax, -16(%rbp)      # [rbp-16] = old_len
    xor %r9, %r9             # r9 = i (pos in s)
    xor %r11, %r11           # r11 = j (pos in dst)
.Lsr_build:
    cmp %r9, %r15            # i >= s_len?
    jbe .Lsr_done_build
    # Try to match old at s[i]
    mov -16(%rbp), %rax      # rax = old_len
    add %r9, %rax            # rax = i + old_len
    cmp %rax, %r15           # i + old_len > s_len?
    jb .Lsr_copy_char
    # Compare old with s[i..i+old_len]
    mov %r12, %rax           # rax = s + i
    add %r9, %rax
    mov %r13, %rcx           # rcx = old
    mov -16(%rbp), %r10      # r10 = old_len (count)
.Lsr_bmatch:
    test %r10, %r10
    jz .Lsr_bfound
    movzbl (%rcx), %edx
    movzbl (%rax), %r8d
    cmp %dl, %r8b
    jne .Lsr_bnomatch
    inc %rcx
    inc %rax
    dec %r10
    jmp .Lsr_bmatch
.Lsr_bfound:
    # Copy new into dst at position j
    mov %rbx, %rdi           # rdi = dst
    add %r11, %rdi           # rdi = dst + j
    mov %r14, %rsi           # rsi = new (src)
    mov -8(%rbp), %rdx       # rdx = new_len
    call flint_memcpy
    # Advance: j += new_len, i += old_len
    mov -8(%rbp), %rax       # rax = new_len
    add %rax, %r11           # j += new_len
    mov -16(%rbp), %rax      # rax = old_len
    add %rax, %r9            # i += old_len
    jmp .Lsr_build
.Lsr_bnomatch:
    # Copy s[i] to dst[j]
.Lsr_copy_char:
    mov %r12, %rax           # rax = s
    add %r9, %rax
    movzbl (%rax), %edx
    mov %rbx, %rax           # rax = dst
    add %r11, %rax
    movb %dl, (%rax)
    inc %r9
    inc %r11
    jmp .Lsr_build
.Lsr_done_build:
    mov %rbx, %rax           # return dst
    jmp .Lsr_ret
.Lsr_empty_old:
    # empty old: return a copy of s
    lea 1(%r15), %rdi        # size = s_len + 1
    call flint_alloc            # rax = dst
    mov %rax, %rbx           # rbx = dst
    mov %r12, %rsi           # src = s
    mov %rbx, %rdi           # dst
    mov %r15, %rdx           # n = s_len
    call flint_memcpy
    mov %rbx, %rax           # return dst
.Lsr_ret:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_str_replace, .-flint_str_replace

    .globl flint_b64_encode
    .type flint_b64_encode, @function
# flint_b64_encode(s): rdi = s. Returns a heap-allocated base64-encoded string.
flint_b64_encode:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = s
    mov %r12, %rdi
    call flint_strlen           # rax = n
    mov %rax, %r13           # r13 = n
    # out_len = (n + 2) / 3 * 4
    lea 2(%r13), %r14
    mov $3, %r15
    xor %rdx, %rdx
    mov %r14, %rax
    div %r15                 # rax = (n+2)/3
    mov $4, %r15
    imul %r15, %rax          # rax = out_len
    lea 1(%rax), %rdi
    call flint_alloc            # rax = dst
    mov %rax, %rbx           # rbx = dst
    lea .Lb64_alpha(%rip), %r14  # r14 = alphabet
    xor %r15, %r15           # r15 = i (pos in s)
    mov $0, -8(%rbp)         # [rbp-8] = j (pos in dst)
.Lb64_enc:
    cmp %r15, %r13           # i >= n?
    jbe .Lb64_done
    # Load up to 3 bytes
    movzbl (%r12, %r15), %eax   # b0
    mov %al, -16(%rbp)      # [rbp-16] = b0
    inc %r15
    cmp %r15, %r13
    jbe .Lb64_one
    movzbl (%r12, %r15), %eax   # b1
    mov %al, -24(%rbp)      # [rbp-24] = b1
    inc %r15
    cmp %r15, %r13
    jbe .Lb64_two
    movzbl (%r12, %r15), %eax   # b2
    mov %al, -32(%rbp)      # [rbp-32] = b2
    inc %r15
    # --- 3-byte path ---
    # c0 = b0 >> 2
    mov -16(%rbp), %eax
    shr $2, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # c1 = ((b0 & 3) << 4) | (b1 >> 4)
    mov -16(%rbp), %eax
    and $3, %al
    shl $4, %al
    mov -24(%rbp), %ecx
    shr $4, %cl
    or %cl, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # c2 = ((b1 & 0xF) << 2) | (b2 >> 6)
    mov -24(%rbp), %eax
    and $0xF, %al
    shl $2, %al
    mov -32(%rbp), %ecx
    shr $6, %cl
    or %cl, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # c3 = b2 & 0x3F
    mov -32(%rbp), %eax
    and $0x3F, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    jmp .Lb64_enc
.Lb64_two:
    # 2-byte path: c0, c1, c2, '='
    mov -16(%rbp), %eax
    shr $2, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    mov -16(%rbp), %eax
    and $3, %al
    shl $4, %al
    mov -24(%rbp), %ecx
    shr $4, %cl
    or %cl, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # c2 = (b1 & 0xF) << 2
    mov -24(%rbp), %eax
    and $0xF, %al
    shl $2, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # c3 = '='
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb $61, (%r10)
    inc -8(%rbp)
    jmp .Lb64_enc
.Lb64_one:
    # 1-byte path: c0, c1, '=', '='
    mov -16(%rbp), %eax
    shr $2, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    mov -16(%rbp), %eax
    and $3, %al
    shl $4, %al
    movzbl %al, %eax
    movzbl (%r14, %rax), %eax
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb $61, (%r10)
    inc -8(%rbp)
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb $61, (%r10)
    inc -8(%rbp)
    jmp .Lb64_enc
.Lb64_done:
    mov %rbx, %rax           # return dst
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_b64_encode, .-flint_b64_encode

    .globl flint_b64_decode
    .type flint_b64_decode, @function
# flint_b64_decode(s): rdi = s (base64 string). Returns a heap-allocated decoded string.
flint_b64_decode:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %r12            # r12 = s
    mov %r12, %rdi
    call flint_strlen           # rax = n
    mov %rax, %r13           # r13 = n
    # out_len = n / 4 * 3 (upper bound)
    mov $4, %r14
    xor %rdx, %rdx
    mov %r13, %rax
    div %r14                 # rax = n/4
    mov $3, %r14
    imul %r14, %rax          # rax = out_len
    lea 1(%rax), %rdi
    call flint_alloc            # rax = dst
    mov %rax, %rbx           # rbx = dst
    lea .Lb64_rev(%rip), %r14  # r14 = reverse lookup table
    xor %r15, %r15           # r15 = i (pos in s)
    mov $0, -8(%rbp)         # [rbp-8] = j (pos in dst)
.Lb64_dec:
    cmp %r15, %r13
    jbe .Lb64_ddone
    # Load 4 chars (0 for missing)
    movzbl (%r12, %r15), %eax
    mov %al, -16(%rbp)
    inc %r15
    cmp %r15, %r13
    jbe .Lb64_dz2
    movzbl (%r12, %r15), %eax
    mov %al, -24(%rbp)
    inc %r15
    cmp %r15, %r13
    jbe .Lb64_dz3
    movzbl (%r12, %r15), %eax
    mov %al, -32(%rbp)
    inc %r15
    cmp %r15, %r13
    jbe .Lb64_dz4
    movzbl (%r12, %r15), %eax
    mov %al, -40(%rbp)
    inc %r15
    jmp .Lb64_dcount
.Lb64_dz4:
    movb $0, -40(%rbp)
    jmp .Lb64_dcount
.Lb64_dz3:
    movb $0, -32(%rbp)
    movb $0, -40(%rbp)
    jmp .Lb64_dcount
.Lb64_dz2:
    movb $0, -24(%rbp)
    movb $0, -32(%rbp)
    movb $0, -40(%rbp)
    jmp .Lb64_dcount
.Lb64_dcount:
    # Count padding ('=' = 61)
    xor %r11, %r11           # r11 = pad_count
    movzbl -32(%rbp), %eax
    cmp $61, %al
    je .Lb64_dp1
    movzbl -40(%rbp), %eax
    cmp $61, %al
    jne .Lb64_dlookup
    inc %r11
    jmp .Lb64_dlookup
.Lb64_dp1:
    inc %r11
    movzbl -40(%rbp), %eax
    cmp $61, %al
    jne .Lb64_dlookup
    inc %r11
.Lb64_dlookup:
    # Look up values
    movzbl -16(%rbp), %eax
    movzbl (%r14, %rax), %eax
    mov %al, -48(%rbp)       # v0
    movzbl -24(%rbp), %eax
    movzbl (%r14, %rax), %eax
    mov %al, -56(%rbp)       # v1
    movzbl -32(%rbp), %eax
    movzbl (%r14, %rax), %eax
    mov %al, -64(%rbp)       # v2
    movzbl -40(%rbp), %eax
    movzbl (%r14, %rax), %eax
    mov %al, -72(%rbp)       # v3
    # b0 = (v0 << 2) | (v1 >> 4)
    movzbl -48(%rbp), %eax
    shl $2, %al
    movzbl -56(%rbp), %ecx
    shr $4, %cl
    or %cl, %al
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
    # b1 = ((v1 & 0xF) << 4) | (v2 >> 2)
    cmp $2, %r11
    jae .Lb64_db2
    movzbl -56(%rbp), %eax
    and $0xF, %al
    shl $4, %al
    movzbl -64(%rbp), %ecx
    shr $2, %cl
    or %cl, %al
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
.Lb64_db2:
    # b2 = ((v2 & 3) << 6) | v3
    cmp $1, %r11
    jae .Lb64_dnext
    movzbl -64(%rbp), %eax
    and $3, %al
    shl $6, %al
    movzbl -72(%rbp), %ecx
    or %cl, %al
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb %al, (%r10)
    inc -8(%rbp)
.Lb64_dnext:
    jmp .Lb64_dec
.Lb64_ddone:
    mov -8(%rbp), %rcx
    mov %rbx, %r10
    add %rcx, %r10
    movb $0, (%r10)
    mov %rbx, %rax
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_b64_decode, .-flint_b64_decode

    .globl flint_log_info
    .type flint_log_info, @function
# flint_log_info(s): rdi = s. Prints "[INFO] s\n" to stdout.
flint_log_info:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    mov %rdi, %r12            # r12 = s
    lea .Llog_info(%rip), %rdi
    mov $7, %rsi             # "[INFO] " = 7 chars... wait, "[INFO] " is 7 chars
    # Actually "[INFO] " is 7 bytes: [ I N F O ] space
    # Let me count: [ = 1, I = 2, N = 3, F = 4, O = 5, ] = 6, space = 7. Yes, 7.
    mov $1, %rdx             # Wait, I need to use write(fd, buf, len)
    # write: rax=1, rdi=fd, rsi=buf, rdx=len
    mov $1, %rax
    mov $1, %rdi             # fd = 1 (stdout)
    # I need to set rsi = buf and rdx = len
    # But I already set rsi and rdx. Let me redo.
    lea .Llog_info(%rip), %rsi
    mov $7, %rdx
    mov $1, %rax
    mov $1, %rdi
    syscall
    # Print s
    mov %r12, %rdi
    call flint_printstr
    # Print newline
    lea .Llog_nl(%rip), %rdi
    mov $1, %rsi
    mov $1, %rdx
    mov $1, %rax
    syscall
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_log_info, .-flint_log_info

    .globl flint_log_warn
    .type flint_log_warn, @function
# flint_log_warn(s): rdi = s. Prints "[WARN] s\n" to stderr.
flint_log_warn:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    mov %rdi, %r12            # r12 = s
    lea .Llog_warn(%rip), %rsi
    mov $7, %rdx
    mov $1, %rax
    mov $2, %rdi             # fd = 2 (stderr)
    syscall
    # Print s to stderr
    mov %r12, %rdi
    call flint_strlen           # rax = len
    mov %rax, %rdx
    mov %r12, %rsi
    mov $1, %rax
    mov $2, %rdi
    syscall
    # Print newline to stderr
    lea .Llog_nl(%rip), %rsi
    mov $1, %rdx
    mov $1, %rax
    mov $2, %rdi
    syscall
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_log_warn, .-flint_log_warn

    .globl flint_log_error
    .type flint_log_error, @function
# flint_log_error(s): rdi = s. Prints "[ERROR] s\n" to stderr.
flint_log_error:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    mov %rdi, %r12            # r12 = s
    lea .Llog_error(%rip), %rsi
    mov $8, %rdx
    mov $1, %rax
    mov $2, %rdi
    syscall
    mov %r12, %rdi
    call flint_strlen
    mov %rax, %rdx
    mov %r12, %rsi
    mov $1, %rax
    mov $2, %rdi
    syscall
    lea .Llog_nl(%rip), %rsi
    mov $1, %rdx
    mov $1, %rax
    mov $2, %rdi
    syscall
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_log_error, .-flint_log_error

    .globl flint_log_debug
    .type flint_log_debug, @function
# flint_log_debug(s): rdi = s. Prints "[DEBUG] s\n" to stdout.
flint_log_debug:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    mov %rdi, %r12            # r12 = s
    lea .Llog_debug(%rip), %rsi
    mov $8, %rdx
    mov $1, %rax
    mov $1, %rdi
    syscall
    mov %r12, %rdi
    call flint_printstr
    lea .Llog_nl(%rip), %rdi
    mov $1, %rsi
    mov $1, %rdx
    mov $1, %rax
    syscall
    pop %r12
    pop %rbx
    leave
    ret
    .size flint_log_debug, .-flint_log_debug

    .globl flint_select
    .type flint_select, @function
# flint_select(n, rfd, wfd, efd, timeout): rdi=n, rsi=rfd, rdx=wfd, rcx=efd, r8=timeout.
# The 4th syscall arg must be in r10; move it before the syscall.
flint_select:
    mov $23, %rax
    mov %rcx, %r10
    syscall
    ret
    .size flint_select, .-flint_select

    .globl flint_poll
    .type flint_poll, @function
# flint_poll(fds, nfds, timeout): rdi=fds, rsi=nfds, rdx=timeout.
flint_poll:
    mov $7, %rax
    syscall
    ret
    .size flint_poll, .-flint_poll

    .globl flint_epoll_create1
    .type flint_epoll_create1, @function
# flint_epoll_create1(flags): rdi=flags.
flint_epoll_create1:
    mov $291, %rax
    syscall
    ret
    .size flint_epoll_create1, .-flint_epoll_create1

    .globl flint_epoll_ctl
    .type flint_epoll_ctl, @function
# flint_epoll_ctl(epfd, op, fd, ev): rdi=epfd, rsi=op, rdx=fd, rcx=ev.
# The 4th syscall arg must be in r10; move it before the syscall.
flint_epoll_ctl:
    mov $232, %rax
    mov %rcx, %r10
    syscall
    ret
    .size flint_epoll_ctl, .-flint_epoll_ctl

    .globl flint_epoll_wait
    .type flint_epoll_wait, @function
# flint_epoll_wait(epfd, events, maxevents, timeout): rdi=epfd, rsi=events, rdx=maxevents, rcx=timeout.
# The 4th syscall arg must be in r10; move it before the syscall.
flint_epoll_wait:
    mov $233, %rax
    mov %rcx, %r10
    syscall
    ret
    .size flint_epoll_wait, .-flint_epoll_wait

    .globl flint_clone
    .type flint_clone, @function
# flint_clone(flags, stack, ptid, tcred, tls): rdi=flags, rsi=stack, rdx=ptid, rcx=tcred, r8=tls.
# The 4th syscall arg must be in r10; move it before the syscall.
flint_clone:
    mov $56, %rax
    mov %rcx, %r10
    syscall
    ret
    .size flint_clone, .-flint_clone

    .globl flint_futex
    .type flint_futex, @function
# flint_futex(uaddr, op, val, val2, val3, val4): rdi=uaddr, rsi=op, rdx=val, rcx=val2, r8=val3, r9=val4.
# The 4th syscall arg must be in r10; move it before the syscall.
flint_futex:
    mov $202, %rax
    mov %rcx, %r10
    syscall
    ret
    .size flint_futex, .-flint_futex

    .globl flint_nanosleep
    .type flint_nanosleep, @function
# flint_nanosleep(seconds, nanos) -> int
# Sleep for the specified duration using nanosleep (syscall 35).
# rdi = seconds, rsi = nanoseconds.
flint_nanosleep:
    sub $16, %rsp           # space for struct timespec (two 64-bit values)
    mov %rdi, (%rsp)       # tv_sec
    mov %rsi, 8(%rsp)      # tv_nsec
    lea (%rsp), %rdi      # ptr to timespec
    mov $35, %rax          # SYS_nanosleep
    syscall
    add $16, %rsp
    xor %eax, %eax         # return 0
    ret
    .size flint_nanosleep, .-flint_nanosleep

    .globl flint_syscall
    .type flint_syscall, @function
# flint_syscall(num, a1, a2, a3, a4, a5) -> int
# Generic syscall escape hatch. Arguments arrive in System V call order
# (rdi=num, rsi=a1, rdx=a2, rcx=a3, r8=a4, r9=a5). The kernel reads the
# number from rax and the syscall args from rdi rsi rdx r10 r8 r9, so
# shift the args down by one and move a4 into r10. syscall clobbers
# rcx and r11.
flint_syscall:
    mov %rdi, %rax    # num -> rax
    mov %rsi, %rdi    # a1
    mov %rdx, %rsi    # a2
    mov %rcx, %rdx    # a3
    mov %r8, %r10     # a4 -> r10 (4th syscall arg)
    mov %r9, %r8      # a5 -> r8  (5th syscall arg)
    syscall
    ret
    .size flint_syscall, .-flint_syscall

    .globl flint_byte_load
    .type flint_byte_load, @function
# flint_byte_load(*byte ptr) -> int
# Loads 1 byte from ptr (rdi), zero-extends to eax.
flint_byte_load:
    movzbl (%rdi), %eax
    ret
    .size flint_byte_load, .-flint_byte_load

    .globl flint_byte_store
    .type flint_byte_store, @function
# flint_byte_store(*byte ptr, int val)
# Stores the low byte of val (rsi) to ptr (rdi).
flint_byte_store:
    mov %sil, (%rdi)
    ret
    .size flint_byte_store, .-flint_byte_store

    .globl flint_short_load
    .type flint_short_load, @function
# flint_short_load(*short ptr) -> int
# Loads 2 bytes (little-endian) from ptr (rdi), zero-extends to eax.
flint_short_load:
    movzwl (%rdi), %eax
    ret
    .size flint_short_load, .-flint_short_load

    .globl flint_short_store
    .type flint_short_store, @function
# flint_short_store(*short ptr, int val)
# Stores the low 2 bytes of val (rsi) to ptr (rdi).
flint_short_store:
    mov %ax, (%rdi)
    ret
    .size flint_short_store, .-flint_short_store

    .globl flint_int_load
    .type flint_int_load, @function
# flint_int_load(*int ptr) -> int
# Loads 4 bytes (little-endian) from ptr (rdi), zero-extends to eax.
flint_int_load:
    movl (%rdi), %eax
    ret
    .size flint_int_load, .-flint_int_load

    .globl flint_int_store
    .type flint_int_store, @function
# flint_int_store(*int ptr, int val)
# Stores the low 4 bytes of val (rsi) to ptr (rdi).
flint_int_store:
    movl %esi, (%rdi)
    ret
    .size flint_int_store, .-flint_int_store

    .section .bss
    .lcomm flint_exception, 8
    .section .text

    .globl flint_throw
    .type flint_throw, @function
# flint_throw() - stores the value in rax into the global exception slot.
flint_throw:
    mov %rax, flint_exception
    ret
    .size flint_throw, .-flint_throw

    .globl flint_exc_clear
    .type flint_exc_clear, @function
# flint_exc_clear() - clears the global exception slot.
flint_exc_clear:
    xor %rax, %rax
    mov %rax, flint_exception
    ret
    .size flint_exc_clear, .-flint_exc_clear

    .globl flint_exc_check
    .type flint_exc_check, @function
# flint_exc_check() -> int - returns 1 if an exception is pending, 0 otherwise.
flint_exc_check:
    mov flint_exception, %rax
    test %rax, %rax
    jz .Lexc_check_zero
    mov $1, %eax
    ret
    .Lexc_check_zero:
    xor %eax, %eax
    ret
    .size flint_exc_check, .-flint_exc_check

    .globl flint_exc_get
    .type flint_exc_get, @function
# flint_exc_get() -> int - returns the pending exception value.
flint_exc_get:
    mov flint_exception, %rax
    ret
    .size flint_exc_get, .-flint_exc_get

    .globl flint_fadd
    .type flint_fadd, @function
# flint_fadd(a, b) -> int: fixed-point add (Q48.16)
flint_fadd:
    add %rsi, %rdi
    mov %rdi, %rax
    ret
    .size flint_fadd, .-flint_fadd

    .globl flint_fsub
    .type flint_fsub, @function
# flint_fsub(a, b) -> int: fixed-point sub (Q48.16)
flint_fsub:
    sub %rsi, %rdi
    mov %rdi, %rax
    ret
    .size flint_fsub, .-flint_fsub

    .globl flint_fmul
    .type flint_fmul, @function
# flint_fmul(a, b) -> int: fixed-point mul (Q48.16)
# result = (a * b) >> 16
flint_fmul:
    mov %rdi, %rax
    mul %rsi
    shr $16, %rdx
    shr $16, %rax
    add %rdx, %rax
    ret
    .size flint_fmul, .-flint_fmul

    .globl flint_fdiv
    .type flint_fdiv, @function
# flint_fdiv(a, b) -> int: fixed-point div (Q48.16)
# result = (a << 16) / b
flint_fdiv:
    mov %rdi, %rax
    shl $16, %rax
    xor %rdx, %rdx
    div %rsi
    ret
    .size flint_fdiv, .-flint_fdiv

    .globl flint_ftoi
    .type flint_ftoi, @function
# flint_ftoi(f) -> int: convert fixed-point to int (round)
flint_ftoi:
    mov %rdi, %rax
    add $32768, %rax  # add 0.5 (2^15) for rounding
    shr $16, %rax
    ret
    .size flint_ftoi, .-flint_ftoi

    .globl flint_itof
    .type flint_itof, @function
# flint_itof(i) -> int: convert int to fixed-point
flint_itof:
    shl $16, %rdi
    mov %rdi, %rax
    ret
    .size flint_itof, .-flint_itof

    .globl flint_fcmp
    .type flint_fcmp, @function
# flint_fcmp(a, b) -> int: compare fixed-point (returns -1, 0, or 1)
flint_fcmp:
    cmp %rsi, %rdi
    jl .Lfcmp_neg
    jg .Lfcmp_pos
    xor %eax, %eax
    ret
    .Lfcmp_neg:
    mov $-1, %rax
    ret
    .Lfcmp_pos:
    mov $1, %eax
    ret
    .size flint_fcmp, .-flint_fcmp

# Thread support: CLONE_VM with custom stack, per-thread pipe for join.
# Supports up to 8 concurrent threads via a fixed pipe-fd table.
    .data
    .globl flint_thread_pipe_read
    .balign 8
flint_thread_pipe_read:
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .globl flint_thread_pipe_write
    .balign 8
flint_thread_pipe_write:
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .globl flint_thread_next_id
    .balign 8
flint_thread_next_id:
    .quad 0
    .globl flint_thread_pid
    .balign 8
flint_thread_pid:
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .globl flint_wait_status
flint_wait_status:
    .quad 0
    .globl flint_thread_result
    .balign 8
flint_thread_result:
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .quad 0; .quad 0; .quad 0; .quad 0
    .globl flint_pipe_byte
flint_pipe_byte:
    .byte 0
    .text

    .globl flint_thread_wrapper
    .type flint_thread_wrapper, @function
# flint_thread_wrapper: entry point for the new thread.
# Stack layout (set up by flint_thread_create):
#   [rsp+0]  = fn  (function pointer)
#   [rsp+8]  = arg (argument)
#   [rsp+16] = tid (thread id, index into pipe table)
flint_thread_wrapper:
    mov 0(%rsp), %r10    # r10 = fn
    mov 8(%rsp), %r11    # r11 = arg
    mov 16(%rsp), %r12   # r12 = tid
    # Call fn(arg)
    mov %r11, %rdi       # arg in rdi
    call *%r10           # call fn(arg); rax = return value
    # The worker may have clobbered r12 (it is a scratch register in the
    # flintc calling style), so re-read the tid from the stack.
    mov 16(%rsp), %r12
    # Store the worker's return value so thread_join can hand it back
    lea flint_thread_result(%rip), %r14
    movq %rax, 0(%r14, %r12, 8)
    # Signal done: write 1 byte to this thread's pipe
    lea flint_thread_pipe_write(%rip), %r13  # r13 = &table
    movq 0(%r13, %r12, 8), %rdi             # rdi = write fd for tid
    lea flint_pipe_byte(%rip), %rsi          # buffer
    mov $1, %rdx               # count = 1
    xor %r10, %r10            # extra args = 0
    xor %r8, %r8
    xor %r9, %r9
    mov $1, %rax              # SYS_write
    syscall
    # Exit the thread
    mov $60, %rax        # SYS_exit
    xor %rdi, %rdi       # exit code = 0
    syscall
    hlt
    .size flint_thread_wrapper, .-flint_thread_wrapper


    .globl flint_thread_create
    .type flint_thread_create, @function
# flint_thread_create(fn, arg) -> int
# Creates a new thread using clone(CLONE_VM). Returns tid on success or -1 on error.
flint_thread_create:
    push %rbp
    mov %rsp, %rbp
    push %rbx             # rbx is callee-saved (the trampoline keeps the
    sub $32, %rsp         # slot index in it); save/restore around our use
    mov %rdi, %rbx        # rbx = fn
    mov %rsi, %r12        # r12 = arg
    # Allocate a tid from the counter (atomic: concurrent thread_creates
    # run in parallel under CLONE_VM, so a plain read-modify-write races)
.Ltid_alloc:
    movq flint_thread_next_id(%rip), %r13  # r13 = candidate tid
    # Bounds check: max 32 threads
    cmp $32, %r13
    jae .Lthread_too_many
    # CAS next_id: r13 -> r13+1 (retry if another process grabbed it first)
    lea flint_thread_next_id(%rip), %rdi
    mov %r13, %rsi                # old
    lea 1(%r13), %rdx            # new
    call flint_atomic_cas
    test %eax, %eax
    jz .Ltid_alloc               # lost the race: retry with the new value
    # Create a pipe for thread synchronization
    lea 16(%rsp), %rdi     # rdi = &pipe_fds (on stack)
    mov $22, %rax          # SYS_pipe
    syscall
    test %rax, %rax
    jne .Lthread_pipe_fail  # rax < 0 on error
    # Save pipe fds (int is 4 bytes; pipe writes fd[0] at +0 and fd[1] at +4)
    movl 16(%rsp), %r14d   # r14 = read fd
    movl 20(%rsp), %r15d   # r15 = write fd
    # Store in per-thread table
    lea flint_thread_pipe_read(%rip), %r10
    movq %r14, 0(%r10, %r13, 8)   # table_read[tid] = read fd
    lea flint_thread_pipe_write(%rip), %r10
    movq %r15, 0(%r10, %r13, 8)   # table_write[tid] = write fd
    # Allocate a new stack (8KB)
    mov $8192, %rdi
    call flint_alloc         # rax = start
    # Stack top = start + 8192
    lea 8192(%rax), %r10   # r10 = stack top
    # Set up the stack (stack grows down, 16-byte aligned):
    mov %rbx, -64(%r10)   # [stack_top-64] = fn
    mov %r12, -56(%r10)   # [stack_top-56] = arg
    mov %r13, -48(%r10)   # [stack_top-48] = tid
    # clone(CLONE_VM, stack, NULL, NULL, NULL)
    mov $0x00000100, %rdi  # CLONE_VM
    lea -64(%r10), %rsi    # stack = stack_top - 64 (16-byte aligned)
    xor %rdx, %rdx         # ptid = NULL
    xor %r10, %r10         # tcred = NULL (4th arg in r10)
    xor %r8, %r8           # tls = NULL (5th arg in r8)
    mov $56, %rax          # SYS_clone
    syscall
    # rax = child pid (parent) or 0 (child)
    test %rax, %rax
    jz .Lthread_child
    # Parent: store child pid for reaping, then return tid
    lea flint_thread_pid(%rip), %r10
    movq %rax, 0(%r10, %r13, 8)   # pid_table[tid] = child pid
    mov -8(%rbp), %rbx
    leave
    mov %r13, %rax
    ret
    .Lthread_child:
    jmp flint_thread_wrapper
    .Lthread_pipe_fail:
    mov -8(%rbp), %rbx
    leave
    movq $-1, %rax
    ret
    .Lthread_too_many:
    mov -8(%rbp), %rbx
    leave
    movq $-1, %rax
    ret
    .size flint_thread_create, .-flint_thread_create

    .globl flint_thread_join
    .type flint_thread_join, @function
# flint_thread_join(tid) -> int
# Wait for a specific thread to finish using a blocking pipe read.
flint_thread_join:
    # rdi = tid (save it)
    mov %rdi, %r12          # r12 = tid
    # Blocking read from the pipe
    lea flint_thread_pipe_read(%rip), %r10   # r10 = &table
    movq 0(%r10, %r12, 8), %rdi             # rdi = read fd for tid
    lea flint_pipe_byte(%rip), %rsi          # buffer
    mov $1, %rdx             # count = 1
    xor %r8, %r8
    xor %r9, %r9
    mov $0, %rax             # SYS_read
    syscall
    # Reap the child: wait4(pid, &status, 0, NULL)
    lea flint_thread_pid(%rip), %r10
    movq 0(%r10, %r12, 8), %rdi             # rdi = child pid for tid
    lea flint_wait_status(%rip), %rsi        # rsi = &status
    xor %rdx, %rdx           # options = 0
    xor %r10, %r10           # rusage = NULL (4th arg)
    xor %r8, %r8
    xor %r9, %r9
    mov $61, %rax            # SYS_wait4
    syscall
    # Return the worker's result value
    lea flint_thread_result(%rip), %r10
    movq 0(%r10, %r12, 8), %rax
    ret
    .size flint_thread_join, .-flint_thread_join

    .globl flint_mutex_lock
    .type flint_mutex_lock, @function
# flint_mutex_lock(addr)
# Lock a mutex using futex(FUTEX_WAIT). The mutex is an int: 0=unlocked, 1=locked.
# Uses atomic CAS to try to acquire, then futex to wait if contended.
flint_mutex_lock:
.Lmutex_try:
    # Try to acquire: CAS 0 -> 1
    mov $1, %r10
    lock cmpxchg %r10, (%rdi)
    jz .Lmutex_acquired
    # CAS failed: check if mutex is still locked
    cmp $1, (%rdi)
    jne .Lmutex_try    # mutex was unlocked, retry
    # Still locked: wait on futex
    # futex(uaddr, FUTEX_WAIT, 1, NULL, NULL, 0)
    mov $202, %rax     # SYS_futex
    # rdi = uaddr (already set)
    mov $0, %rsi      # op = FUTEX_WAIT
    mov $1, %rdx      # val = 1
    xor %r10, %r10    # timeout = NULL
    xor %r8, %r8      # uaddr2 = NULL
    xor %r9, %r9      # val3 = 0
    syscall
    # Woken up: retry the CAS
    jmp .Lmutex_try
    .Lmutex_acquired:
    ret
    .size flint_mutex_lock, .-flint_mutex_lock

    .globl flint_mutex_unlock
    .type flint_mutex_unlock, @function
# flint_mutex_unlock(addr)
# Unlock a mutex using futex(FUTEX_WAKE). Set the mutex to 0 and wake one waiter.
flint_mutex_unlock:
    mov $0, (%rdi)  # set mutex to 0 (unlocked)
    # futex(uaddr, FUTEX_WAKE, 1, NULL, NULL, 0)
    mov $202, %rax  # SYS_futex
    # rdi = uaddr (already set)
    mov $1, %rsi    # op = FUTEX_WAKE
    mov $1, %rdx    # val = 1 (wake 1 thread)
    xor %r10, %r10  # timeout = NULL
    xor %r8, %r8    # uaddr2 = NULL
    xor %r9, %r9    # val3 = 0
    syscall
    ret
    .size flint_mutex_unlock, .-flint_mutex_unlock

    .globl flint_atomic_cas
    .type flint_atomic_cas, @function
# flint_atomic_cas(addr, old, new) -> int
# Compare-and-swap: if *addr == old, set *addr = new and return 1; else return 0.
flint_atomic_cas:
    mov %rsi, %rax  # old value in rax (cmpxchg compares against rax)
    # rdx = new value (3rd arg)
    lock cmpxchg %rdx, (%rdi)
    # If ZF=1, the swap succeeded
    setz %al
    movzbl %al, %eax
    ret
    .size flint_atomic_cas, .-flint_atomic_cas

    .globl flint_fn_addr
    .type flint_fn_addr, @function
# flint_fn_addr(addr) -> int
# Returns the function address (identity - the address is already known at compile time).
# This is a placeholder; the actual function address is passed as the argument.
flint_fn_addr:
    mov %rdi, %rax
    ret
    .size flint_fn_addr, .-flint_fn_addr

    .globl flint_call_ptr
    .type flint_call_ptr, @function
# flint_call_ptr(fn_addr, arg) -> int
# Calls a function via its address. The function takes one int argument and returns int.
flint_call_ptr:
    mov %rdi, %rax  # fn address in rax
    mov %rsi, %rdi  # arg in rdi
    call *%rax      # indirect call
    ret
    .size flint_call_ptr, .-flint_call_ptr

    .globl flint_json_get
    .type flint_json_get, @function
# flint_json_get(json, key) -> *byte
# Extracts the value for a key from a JSON object.
# v1: simple implementation that searches for "key": in the json string
# and returns the value after the colon (up to comma, brace, or bracket).
flint_json_get:
    push %rbp
    mov %rsp, %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    
    # rdi = json, rsi = key
    # Step 1: Calculate key length
    mov %rsi, %r12
    xor %r13, %r13
    .Ljson_keylen:
    movzbl (%r12, %r13), %eax
    test %al, %al
    jz .Ljson_keylen_done
    inc %r13
    jmp .Ljson_keylen
    .Ljson_keylen_done:
    mov %r13, %r14  # key_len
    
    # Step 2: Search for the key in the json string
    # Look for: " + key + " followed by optional whitespace and a colon
    mov %rdi, %r12  # json string
    xor %r13, %r13  # i = 0
    .Ljson_search:
    movzbl (%r12, %r13), %eax
    test %al, %al
    jz .Ljson_notfound
    cmp $34, %al  # '"'
    jne .Ljson_search_next
    # Check if the next key_len chars match the key
    mov %r13, %r15  # pos = i
    add $1, %r15   # skip the opening quote
    xor %rax, %rax  # j = 0
    .Ljson_keymatch:
    cmp %r14, %rax
    jae .Ljson_keymatch_done
    movzbl (%r12, %r15, 1), %edx
    movzbl (%rsi, %rax, 1), %ecx
    cmp %cl, %dl
    jne .Ljson_search_next
    inc %rax
    inc %r15
    jmp .Ljson_keymatch
    .Ljson_keymatch_done:
    # Found the key; now find the closing quote and colon
    add $1, %r15  # skip closing quote
    .Ljson_findcolon:
    movzbl (%r12, %r15), %eax
    test %al, %al
    jz .Ljson_notfound
    cmp $58, %al  # ':'
    je .Ljson_found
    inc %r15
    jmp .Ljson_findcolon
    .Ljson_search_next:
    inc %r13
    jmp .Ljson_search
    
    .Ljson_notfound:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    # Return pointer to empty string
    lea .Ljson_empty(%rip), %rax
    ret
    
    .Ljson_found:
    # r15 points to the colon; skip to the value
    inc %r15
    # Skip whitespace
    .Ljson_skipws:
    movzbl (%r12, %r15), %eax
    cmp $32, %al
    je .Ljson_skipws_adv
    cmp $9, %al
    je .Ljson_skipws_adv
    cmp $10, %al
    je .Ljson_skipws_adv
    cmp $13, %al
    je .Ljson_skipws_adv
    jmp .Ljson_skipws_done
    .Ljson_skipws_adv:
    inc %r15
    jmp .Ljson_skipws
    .Ljson_skipws_done:
    # r15 now points to the start of the value
    # Check if it's a string value
    cmp $34, %al
    je .Ljson_strval
    # Non-string value: extract up to comma, closing brace, or closing bracket
    mov %r15, %r13  # start = r15
    .Ljson_nonstr:
    movzbl (%r12, %r13), %eax
    test %al, %al
    jz .Ljson_nonstr_done
    cmp $44, %al  # ','
    je .Ljson_nonstr_done
    cmp $125, %al  # '}'
    je .Ljson_nonstr_done
    cmp $93, %al  # ']'
    je .Ljson_nonstr_done
    inc %r13
    jmp .Ljson_nonstr
    .Ljson_nonstr_done:
    # Allocate and copy the value
    sub %r15, %r13  # len = end - start
    mov %r13, %rdi  # size for flint_alloc
    call flint_alloc  # rax = allocated pointer
    lea (%r12, %r15), %rsi  # src pointer
    xor %rcx, %rcx  # i = 0
    .Ljson_copy:
    cmp %r13, %rcx
    jae .Ljson_copy_done
    movzbl (%rsi, %rcx, 1), %edx
    mov %dl, (%rax, %rcx, 1)
    inc %rcx
    jmp .Ljson_copy
    .Ljson_copy_done:
    xor %al, %al
    mov %al, (%rax, %r13, 1)  # NUL terminate
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    ret  # rax = allocated pointer
    
    .Ljson_strval:
    # String value: find the closing quote
    inc %r15  # skip opening quote
    mov %r15, %r13  # start = r15
    .Ljson_strend:
    movzbl (%r12, %r13), %eax
    test %al, %al
    jz .Ljson_strerr
    cmp $92, %al  # backslash
    je .Ljson_strskip
    cmp $34, %al  # closing quote
    je .Ljson_strdone
    inc %r13
    jmp .Ljson_strend
    .Ljson_strskip:
    add $2, %r13
    jmp .Ljson_strend
    .Ljson_strerr:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    lea .Ljson_empty(%rip), %rax
    ret
    .Ljson_strdone:
    # r13 points to the closing quote; the value is from r15 to r13
    sub %r15, %r13  # len = end - start
    mov %r13, %rdi  # size for flint_alloc
    call flint_alloc  # rax = allocated pointer
    lea (%r12, %r15), %rsi  # src pointer
    xor %rcx, %rcx  # i = 0
    .Ljson_strcopy:
    cmp %r13, %rcx
    jae .Ljson_strcopy_done
    movzbl (%rsi, %rcx, 1), %edx
    mov %dl, (%rax, %rcx, 1)
    inc %rcx
    jmp .Ljson_strcopy
    .Ljson_strcopy_done:
    xor %al, %al
    mov %al, (%rax, %r13, 1)  # NUL terminate
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    ret  # rax = allocated pointer
    .size flint_json_get, .-flint_json_get

    .globl flint_json_geti
    .type flint_json_geti, @function
# flint_json_geti(json, key) -> int
# Extracts an integer value for a key from a JSON object.
# Calls flint_json_get and then parses the result as an integer.
flint_json_geti:
    # rdi = json, rsi = key
    call flint_json_get
    # rax = pointer to the value string
    # Parse as integer using flint_atoi
    mov %rax, %rdi
    call flint_atoi
    ret
    .size flint_json_geti, .-flint_json_geti

    .globl flint_str_format
    .type flint_str_format, @function
# flint_str_format(fmt, a, b, c, d) -> *byte
# rdi = fmt, rsi = a, rdx = b, r10 = c, r8 = d
# Supports %d (int), %s (*byte), %b (int as bool)
# All args stored on stack at fixed offsets from rbp.
# r12=fmt, r13=a, rbx=b, r15=c, d at -56(%rbp), divisor at -64(%rbp)
flint_str_format:
    push %rbp
    mov %rsp, %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    push %rbx
    sub $32, %rsp
    # Save args (System V: 4th arg in rcx, 5th in r8)
    mov %rdi, %r12
    mov %rsi, %r13
    mov %rdx, %rbx
    mov %rcx, %r15
    mov %r8, -56(%rbp)
    mov $10, -64(%rbp)
    mov %r15, -72(%rbp)
    # First pass: calculate result length in r11
    xor %r11, %r11
    xor %rcx, %rcx
    mov %r12, %rsi
    .Lfmt_len:
    movzbl (%rsi), %edx
    test %dl, %dl
    jz .Lfmt_len_done
    cmp $37, %dl
    je .Lfmt_spec
    inc %r11
    inc %rsi
    jmp .Lfmt_len
    .Lfmt_spec:
    inc %rsi
    movzbl (%rsi), %edx
    cmp $100, %dl
    je .Lfmt_spec_len_d
    cmp $115, %dl
    je .Lfmt_spec_len_s
    cmp $98, %dl
    je .Lfmt_spec_len_b
    inc %r11
    inc %rsi
    jmp .Lfmt_len
    .Lfmt_spec_len_d:
    add $20, %r11
    inc %rcx
    inc %rsi
    jmp .Lfmt_len
    .Lfmt_spec_len_s:
    call .Lfmt_get_arg
    mov %rax, %r10
    xor %r9, %r9
    .Lfmt_spec_len_s_loop:
    movzbl (%r10, %r9), %eax
    test %al, %al
    jz .Lfmt_spec_len_s_done
    inc %r9
    jmp .Lfmt_spec_len_s_loop
    .Lfmt_spec_len_s_done:
    add %r9, %r11
    inc %rcx
    inc %rsi
    jmp .Lfmt_len
    .Lfmt_spec_len_b:
    inc %r11
    inc %rcx
    inc %rsi
    jmp .Lfmt_len
    .Lfmt_len_done:
    inc %r11
    mov -72(%rbp), %r15
    mov %r11, %rdi
    call flint_alloc
    mov %rax, %r14
    # Second pass: fill the result
    xor %rcx, %rcx
    xor %r9, %r9
    mov %r12, %rsi
    .Lfmt_fill:
    movzbl (%rsi), %edx
    test %dl, %dl
    jz .Lfmt_fill_done
    cmp $37, %dl
    je .Lfmt_fill_spec
    mov %dl, (%r14, %r9, 1)
    inc %r9
    inc %rsi
    jmp .Lfmt_fill
    .Lfmt_fill_spec:
    inc %rsi
    movzbl (%rsi), %edx
    cmp $100, %dl
    je .Lfmt_fill_d
    cmp $115, %dl
    je .Lfmt_fill_s
    cmp $98, %dl
    je .Lfmt_fill_b
    mov %dl, (%r14, %r9, 1)
    inc %r9
    inc %rsi
    jmp .Lfmt_fill
    .Lfmt_fill_d:
    call .Lfmt_get_arg
    mov %rax, %r10
    test %r10, %r10
    jns .Lfmt_fill_d_pos
    neg %r10
    mov $45, %dl
    mov %dl, (%r14, %r9, 1)
    inc %r9
    .Lfmt_fill_d_pos:
    mov %r9, %r11
    .Lfmt_fill_d_div:
    # Software division by 10: r10 = value, result in r8 (quotient), r10 (remainder)
    xor %r8, %r8
    .Lfmt_fill_d_div_loop:
    cmp $10, %r10
    jb .Lfmt_fill_d_div_done
    sub $10, %r10
    inc %r8
    jmp .Lfmt_fill_d_div_loop
    .Lfmt_fill_d_div_done:
    lea 48(%r10), %r10
    # r10 = digit char, r8 = new value
    mov %r10, %rax
    mov %al, (%r14, %r9, 1)
    inc %r9
    mov %r8, %r10
    test %r10, %r10
    jnz .Lfmt_fill_d_div
    # rcx (arg index) is not clobbered by the div loop; keep it
    # r9 = end (start + num_digits)
    mov %r9, %r8
    dec %r9
    .Lfmt_fill_d_rev:
    cmp %r11, %r9
    jbe .Lfmt_fill_d_done
    movzbl (%r14, %r11, 1), %eax
    movzbl (%r14, %r9, 1), %edx
    mov %al, (%r14, %r9, 1)
    mov %dl, (%r14, %r11, 1)
    inc %r11
    dec %r9
    jmp .Lfmt_fill_d_rev
    .Lfmt_fill_d_done:
    mov %r8, %r9
    inc %rcx
    inc %rsi
    jmp .Lfmt_fill
    .Lfmt_fill_s:
    call .Lfmt_get_arg
    mov %rax, %r10
    xor %r8, %r8
    .Lfmt_fill_s_loop:
    movzbl (%r10, %r8), %eax
    test %al, %al
    jz .Lfmt_fill_s_done
    mov %al, (%r14, %r9, 1)
    inc %r9
    inc %r8
    jmp .Lfmt_fill_s_loop
    .Lfmt_fill_s_done:
    inc %rcx
    inc %rsi
    jmp .Lfmt_fill
    .Lfmt_fill_b:
    call .Lfmt_get_arg
    test %rax, %rax
    jz .Lfmt_fill_b_zero
    mov $49, %dl
    jmp .Lfmt_fill_b_store
    .Lfmt_fill_b_zero:
    mov $48, %dl
    .Lfmt_fill_b_store:
    mov %dl, (%r14, %r9, 1)
    inc %r9
    inc %rcx
    inc %rsi
    jmp .Lfmt_fill
    .Lfmt_fill_done:
    xor %al, %al
    mov %al, (%r14, %r9, 1)
    mov %r14, %rax
    add $32, %rsp
    pop %rbx
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    leave
    ret
    .size flint_str_format, .-flint_str_format
# Helper: get the arg value for the current arg_idx (in rcx)
# r13=a, rbx=b, r15=c, d at -56(%rbp)
.Lfmt_get_arg:
    test %rcx, %rcx
    jz .Lfmt_get_arg_a
    cmp $1, %rcx
    je .Lfmt_get_arg_b
    cmp $2, %rcx
    je .Lfmt_get_arg_c
    mov -56(%rbp), %rax
    ret
    .Lfmt_get_arg_a:
    mov %r13, %rax
    ret
    .Lfmt_get_arg_b:
    mov %rbx, %rax
    ret
    .Lfmt_get_arg_c:
    mov %r15, %rax
    ret

    .section .rodata
    .Lb64_alpha:
    .string "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    .Lb64_rev:
    # 256-byte reverse lookup table for base64 decoding
    # Index = ASCII char, value = 6-bit value (or 255 for invalid)
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,62,255,255,255,63,62  # '+'=62, '/'=63
    .byte 52,53,54,55,56,57,58,59,60,61,255,255,255,255,255,255  # '0'-'9' = 52-61
    .byte 255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14  # 'A'-'N' = 0-13
    .byte 15,16,17,18,19,20,21,22,23,24,25,255,255,255,255,255  # 'O'-'Z' = 14-25
    .byte 255,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40  # 'a'-'n' = 26-39
    .byte 41,42,43,44,45,46,47,48,49,50,51,255,255,255,255,255  # 'o'-'z' = 40-51
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .byte 255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
    .Llog_info:
    .string "[INFO] "
    .Llog_warn:
    .string "[WARN] "
    .Llog_error:
    .string "[ERROR] "
    .Llog_debug:
    .string "[DEBUG] "
    .Ljson_empty:
    .string ""
    .Llog_nl:
    .string "\n"
    .Lprint_nl:
    .string "\n"

    .section .text
    # Signed division by absolute value: this toolchain's cltd only
    # sign-extends the low 32 bits and idiv traps whenever rdx != 0, so
    # signed / and % always go through here (unsigned div on absolute
    # values). in: rdi = a, rsi = d; out: rax = a / d (truncated).
    # No register other than rax is clobbered.
    .globl flint_sdiv
    .type flint_sdiv, @function
flint_sdiv:
    push %rcx
    push %r8
    push %r9
    push %r10
    push %r11
    push %rdx
    movq %rdi, %rcx
    movq %rsi, %r8
    movq %rcx, %r9
    sarq $63, %r9              # sa
    movq %r8, %r10
    sarq $63, %r10             # sd
    movq %r9, %r11
    xorq %r10, %r11            # r11 = neg flag (-1 or 0)
    xorq %r9, %rcx
    subq %r9, %rcx             # rcx = |a|
    xorq %r10, %r8
    subq %r10, %r8             # r8 = |d|
    xorq %rdx, %rdx
    movq %rcx, %rax
    divq %r8                   # rax = q, rdx = rem
    xorq %r11, %rax
    subq %r11, %rax            # apply sign (wraps correctly for 2^63)
    pop %rdx
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rcx
    ret
    .size flint_sdiv, .-flint_sdiv

    # in: rdi = a, rsi = d; out: rax = a % d (truncated).
    # No register other than rax is clobbered.
    .globl flint_smod
    .type flint_smod, @function
flint_smod:
    push %rcx
    push %r8
    push %r9
    push %r10
    push %r11
    push %rdx
    movq %rdi, %rcx
    movq %rsi, %r8
    movq %rcx, %r9
    sarq $63, %r9              # sa
    movq %r8, %r10
    sarq $63, %r10             # sd
    movq %r9, %r11
    xorq %r10, %r11            # r11 = neg flag
    xorq %r9, %rcx
    subq %r9, %rcx             # rcx = |a|
    xorq %r10, %r8
    subq %r10, %r8             # r8 = |d|
    xorq %rdx, %rdx
    movq %rcx, %rax
    divq %r8                   # rax = q, rdx = rem
    xorq %r11, %rdx
    subq %r11, %rdx            # apply sign to remainder
    movq %rdx, %rax
    pop %rdx
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rcx
    ret
    .size flint_smod, .-flint_smod

    .globl flint_mmap
    .type flint_mmap, @function
# flint_mmap(addr, len, prot, flags, fd, offset) -> ptr
# Args arrive in System V order: rdi=addr, rsi=len, rdx=prot, rcx=flags, r8=fd, r9=offset.
# The kernel reads rdi rsi rdx r10 r8 r9, so shift the 4th arg (flags) from rcx to r10.
flint_mmap:
    mov $9, %rax              # SYS_mmap
    mov %rcx, %r10           # flags -> r10 (4th syscall arg)
    syscall
    ret
    .size flint_mmap, .-flint_mmap

    .globl flint_munmap
    .type flint_munmap, @function
# flint_munmap(addr, len) -> int  (SYS_munmap; rdi=addr, rsi=len already in place)
flint_munmap:
    mov $11, %rax            # SYS_munmap
    syscall
    ret
    .size flint_munmap, .-flint_munmap

    # ------------------------------------------------------------------
    # Task support for `async` functions. A shared table (CLONE_VM) of 32
    # entries, each 7 slots: slot 0 = function pointer, slots 1-6 = the
    # arguments (a method's `this` is slot 1, i.e. ARGREGS[0]). A free
    # flag per entry (0 = free, 1 = in use) is claimed with an atomic CAS.
    # ------------------------------------------------------------------
    .bss
    .globl flint_task_table
    .type flint_task_table, @object
    flint_task_table:
    .zero 32 * 7 * 8
    .size flint_task_table, 32 * 7 * 8
    .globl flint_task_free
    .type flint_task_free, @object
    flint_task_free:
    .zero 32
    .size flint_task_free, 32
    .text

    .globl flint_task_alloc
    .type flint_task_alloc, @function
# flint_task_alloc(a1, a2, a3, a4, a5, a6) -> int
# Takes the argument slots in rdi rsi rdx rcx r8 r9, atomically claims a
# free table entry, writes the argument slots into it (slots 1-6; slot 0
# is the function pointer, written by the caller afterwards), and returns
# the entry index. If all 32 entries are in use, sleeps 100us and retries.
flint_task_alloc:
    push %rbp
    mov %rsp, %rbp
    sub $168, %rsp
    mov %rdi, 120(%rsp)
    mov %rsi, 128(%rsp)
    mov %rdx, 136(%rsp)
    mov %rcx, 144(%rsp)
    mov %r8, 152(%rsp)
    mov %r9, 160(%rsp)
.Ltask_scan:
    xor %r10, %r10
.Ltask_try:
    lea flint_task_free(%rip), %r13
    movzbl 0(%r13, %r10), %r11d
    test %r11, %r11
    jnz .Ltask_next
    # Claim the slot: atomic CAS 0 -> 1 (fast path lost the race -> skip)
    lea 0(%r13, %r10), %rdi
    xor %esi, %esi
    mov $1, %edx
    call flint_atomic_cas
    test %eax, %eax
    jz .Ltask_next
    # Claimed
    jmp .Ltask_got
.Ltask_next:
    lea 1(%r10), %r10
    cmp $32, %r10
    jb .Ltask_try
    # All slots in use: back off and rescan
    xor %edi, %edi
    mov $100000, %esi       # 100us
    call flint_nanosleep
    jmp .Ltask_scan
 .Ltask_got:
    # Base of the claimed entry: table + slot*56 (each entry is 7 qwords).
    # (x86 index scales are only 1/2/4/8, so the 56-byte stride needs an imul.)
    lea flint_task_table(%rip), %r13
    mov %r10, %r12
    imul $56, %r12
    add %r13, %r12          # r12 = base of the entry
    mov 120(%rsp), %r14
    movq %r14, 8(%r12)
    mov 128(%rsp), %r14
    movq %r14, 16(%r12)
    mov 136(%rsp), %r14
    movq %r14, 24(%r12)
    mov 144(%rsp), %r14
    movq %r14, 32(%r12)
    mov 152(%rsp), %r14
    movq %r14, 40(%r12)
    mov 160(%rsp), %r14
    movq %r14, 48(%r12)
    leave
    mov %r10, %rax
    ret
    .size flint_task_alloc, .-flint_task_alloc

    .globl flint_task_trampoline
    .type flint_task_trampoline, @function
# flint_task_trampoline(slot): called by flint_thread_wrapper with the
# table entry index in rdi. Loads the entry's argument slots into the
# argument registers (slot 1 = rdi = first arg / this), calls the
# function pointer (slot 0); the return value is captured by the wrapper
# and handed to flint_thread_join. Afterwards the free flag is cleared so
# the entry can be reused. The slot index is kept in rbx because the
# worker function may clobber every other scratch register (the wrapper
# itself only needs r12, which the trampoline does not touch).
flint_task_trampoline:
    lea flint_task_table(%rip), %r13
    mov %rdi, %rbx        # slot index (survives the worker call)
    # Base of the entry: table + slot*56 (each entry is 7 qwords).
    mov %rbx, %r14
    imul $56, %r14
    add %r13, %r14         # r14 = base of the entry
    mov 8(%r14), %rdi
    mov 16(%r14), %rsi
    mov 24(%r14), %rdx
    mov 32(%r14), %rcx
    mov 40(%r14), %r8
    mov 48(%r14), %r9
    mov 0(%r14), %r10   # function pointer
    call *%r10
    # Free the entry for the next task
    lea flint_task_free(%rip), %r11
    movb $0, 0(%r11, %rbx)
    ret
    .size flint_task_trampoline, .-flint_task_trampoline
