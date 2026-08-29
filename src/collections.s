# flint collections runtime: list, queue, hashset, hashmap (dictionary).
# Linked in with generated user code. No libc. Depends on intrinsics.s:
#   flint_alloc(n)          -> rax  (clobbers rax rcx rdx rsi rdi r8 r9 r10 r11)
#   flint_memcpy(d, s, n)   -> rdi = d + n (clobbers rax rcx rdx rsi rdi r10)
#   flint_strcmp(a, b) -> 0 on equal (clobbers rax rcx rdx rsi rdi)
#
# Only rbx r12 r13 r14 r15 survive a runtime call; every function that uses
# them saves and restores them. Bucket i (16 bytes) lives at base + 16*i,
# entry i (32 bytes) at base + 32*i, computed via shift (no scale > 8).
#
# Object layouts (all heap, slot 0 = size, 8-byte slots, power-of-two caps):
#   list:    [0] len   [1] cap   [2] data ptr
#   queue:   [0] count [1] cap   [2] data ptr   [3] head
#   hashset: [0] count [1] cap   [2] buckets ptr   (bucket = 16 bytes: val, flag)
#   hashmap: [0] count [1] cap   [2] entries ptr   (entry  = 32 bytes: key, kflag, val, vflag)
# Entry flags: 0 = empty, 1 = int, 2 = string, 3 = deleted (tombstone).
# String values are compared with flint_strcmp; ints with ==.

    .section .text

# ------------------------------------------------------------------- hashing
# flint_hash_val(value, flag) -> rax. Clobbers rax rcx rdx r8.
    .globl flint_hash_val
    .type flint_hash_val, @function
flint_hash_val:
    cmpq $2, %rsi
    je .hv_str
    # int: splitmix64 finalizer
    mov %rdi, %rax
    movabs $0x9E3779B97F4A7C15, %rcx
    add %rcx, %rax
    mov %rax, %rcx
    shr $30, %rcx
    xor %rcx, %rax
    movabs $0xBF58476D1CE4E5B9, %rcx
    imul %rcx, %rax
    mov %rax, %rcx
    shr $27, %rcx
    xor %rcx, %rax
    movabs $0x94D049BB133111EB, %rcx
    imul %rcx, %rax
    mov %rax, %rcx
    shr $31, %rcx
    xor %rcx, %rax
    ret
.hv_str:
    # string: FNV-1a 64 over the contents (r8 = prime, rcx = offset)
    movabs $0xcbf29ce484222325, %rax
    movabs $0x100000001b3, %r8
    xor %rcx, %rcx
.hv_fn:
    movzbl (%rdi, %rcx), %edx
    test %dl, %dl
    jz .hv_end
    xor %edx, %eax
    imul %r8, %rax
    inc %rcx
    jmp .hv_fn
.hv_end:
    ret
    .size flint_hash_val, .-flint_hash_val

# ---------------------------------------------------------------------- list
    .globl flint_list_new
    .type flint_list_new, @function
# flint_list_new(n) -> rax. Capacity starts at max(8, n).
flint_list_new:
    push %r12
    push %r14
    mov $8, %r10
    cmp $8, %rdi
    jbe .ln_cap
    mov %rdi, %r10
.ln_cap:
    mov %r10, %r14               # cap
    mov %r14, %rdi
    shl $3, %rdi
    call flint_alloc               # data buffer
    mov %rax, %r12
    mov $32, %rdi
    call flint_alloc               # header
    xor %ecx, %ecx
    movq %rcx, (%rax)            # len = 0
    movq %r14, 8(%rax)           # cap
    movq %r12, 16(%rax)          # data
    pop %r14
    pop %r12
    ret
    .size flint_list_new, .-flint_list_new

    .globl flint_list_add
    .type flint_list_add, @function
# flint_list_add(list, value): append, doubling the buffer when full (old leaks).
flint_list_add:
    push %rbx
    push %r12
    push %r14
    mov %rdi, %rbx               # list
    mov %rsi, %r14               # value
    movq 8(%rbx), %r10           # cap
    cmp %r10, (%rbx)
    jb .la_store                 # len < cap
    shl $1, %r10                 # new cap = cap * 2
    test %r10, %r10
    jnz .la_grow
    mov $8, %r10
.la_grow:
    mov %r10, %r12               # r12 = new cap
    mov %r12, %rdi
    shl $3, %rdi
    call flint_alloc               # new buffer
    movq 16(%rbx), %rsi          # old data
    movq (%rbx), %rdx
    shl $3, %rdx                 # len * 8
    mov %rax, %rdi               # dst = new buffer
    call flint_memcpy              # rdi = new + len*8
    sub %rdx, %rdi
    movq %rdi, 16(%rbx)          # data = new
    movq %r12, 8(%rbx)           # cap = new cap
.la_store:
    movq 16(%rbx), %rax
    movq (%rbx), %rcx
    lea (%rax, %rcx, 8), %rax
    mov %r14, (%rax)
    movq (%rbx), %rax
    addq $1, %rax
    movq %rax, (%rbx)
    pop %r14
    pop %r12
    pop %rbx
    ret
    .size flint_list_add, .-flint_list_add

    .globl flint_list_get
    .type flint_list_get, @function
# flint_list_get(list, i) -> rax (element value; no bounds check in v1)
flint_list_get:
    movq 16(%rdi), %rax
    movq (%rax, %rsi, 8), %rax
    ret
    .size flint_list_get, .-flint_list_get

    .globl flint_list_set
    .type flint_list_set, @function
# flint_list_set(list, i, value)
flint_list_set:
    movq 16(%rdi), %rax
    mov %rdx, (%rax, %rsi, 8)
    ret
    .size flint_list_set, .-flint_list_set

    .globl flint_list_addr
    .type flint_list_addr, @function
# flint_list_addr(list, i) -> rax (address of slot i, for stores)
flint_list_addr:
    movq 16(%rdi), %rax
    lea (%rax, %rsi, 8), %rax
    ret
    .size flint_list_addr, .-flint_list_addr

    .globl flint_list_size
    .type flint_list_size, @function
flint_list_size:
    movq (%rdi), %rax
    ret
    .size flint_list_size, .-flint_list_size

    .globl flint_list_remove
    .type flint_list_remove, @function
# flint_list_remove(list, i): shift tail left, len--. No-op when i >= len.
flint_list_remove:
    push %rbx
    mov %rdi, %rbx
    movq (%rbx), %rax            # len
    cmp %rax, %rsi               # i - len
    jae .lr_done                 # i >= len: no-op
    movq 16(%rbx), %r10          # data
    movq (%rbx), %rdx            # len
    subq %rsi, %rdx
    dec %rdx                     # count = len - i - 1
    test %rdx, %rdx
    jz .lr_pop
    lea (%r10, %rsi, 8), %rdi    # dst = data + i*8
    lea 8(%r10, %rsi, 8), %rsi   # src = data + (i+1)*8 (dst < src: forward ok)
    mov %rdx, %rax
    shl $3, %rax
    mov %rax, %rdx
    call flint_memcpy
.lr_pop:
    movq (%rbx), %rax
    dec %rax
    movq %rax, (%rbx)
.lr_done:
    pop %rbx
    ret
    .size flint_list_remove, .-flint_list_remove

    .globl flint_list_contains
    .type flint_list_contains, @function
# flint_list_contains(list, value, flag) -> 1/0 (strings compared by content)
flint_list_contains:
    push %rbx
    push %r12
    mov %rdi, %rbx
    movq 16(%rbx), %r12          # data
    movq (%rbx), %r11            # len
    xor %r10, %r10               # i
.lc_loop:
    cmp %r11, %r10
    jae .lc_no
    movq (%r12, %r10, 8), %rax
    cmpq $1, %rdx
    je .lc_eq                    # int (flag 1)
    mov %rax, %rdi
    call flint_strcmp              # %rsi = value
    test %rax, %rax
    jnz .lc_next
    jmp .lc_yes
.lc_eq:
    cmp %rsi, %rax
    je .lc_yes
.lc_next:
    inc %r10
    jmp .lc_loop
.lc_yes:
    mov $1, %rax
    jmp .lc_done
.lc_no:
    xor %rax, %rax
.lc_done:
    pop %r12
    pop %rbx
    ret
    .size flint_list_contains, .-flint_list_contains

    .globl flint_list_clear
    .type flint_list_clear, @function
flint_list_clear:
    movq $0, (%rdi)
    ret
    .size flint_list_clear, .-flint_list_clear

# --------------------------------------------------------------------- queue
    .globl flint_queue_new
    .type flint_queue_new, @function
# flint_queue_new(n) -> rax. Capacity starts at the next power of two >= max(8, n).
flint_queue_new:
    push %r12
    push %r14
    mov $8, %r10
    cmp $8, %rdi
    jbe .qn_pow
    mov %rdi, %r10
.qn_pow:
    mov $8, %rax
.qn_l:
    cmp %r10, %rax
    jae .qn_d
    shl $1, %rax
    jmp .qn_l
.qn_d:
    mov %rax, %r14               # cap
    mov %r14, %rdi
    shl $3, %rdi
    call flint_alloc               # data buffer
    mov %rax, %r12
    mov $32, %rdi
    call flint_alloc               # header
    xor %ecx, %ecx
    movq %rcx, (%rax)            # count = 0
    movq %r14, 8(%rax)           # cap
    movq %r12, 16(%rax)          # data
    movq %rcx, 24(%rax)          # head = 0
    pop %r14
    pop %r12
    ret
    .size flint_queue_new, .-flint_queue_new

    .globl flint_queue_push
    .type flint_queue_push, @function
# flint_queue_push(q, value): enqueue at the tail; doubles when full.
flint_queue_push:
    push %rbx
    push %r12
    push %r14
    mov %rdi, %rbx               # q
    mov %rsi, %r14               # value
    movq 8(%rbx), %r10
    cmp %r10, (%rbx)
    jb .qp_store                 # count < cap
    shl $1, %r10                 # new cap
    mov %r10, %r12               # r12 = new cap
    mov %r12, %rdi
    shl $3, %rdi
    call flint_alloc
    movq 16(%rbx), %r11          # old data
    movq 24(%rbx), %rcx
    lea (%r11, %rcx, 8), %rsi    # src = old + head*8
    movq (%rbx), %rdx
    mov %rax, %rdi               # dst = new data
    call flint_memcpy              # rdi = new + count*8
    sub %rdx, %rdi
    movq %rdi, 16(%rbx)
    movq %r12, 8(%rbx)
    movq $0, 24(%rbx)            # head = 0
.qp_store:
    movq 8(%rbx), %r10
    dec %r10                     # mask = cap - 1
    movq 24(%rbx), %rax
    movq (%rbx), %rcx
    lea (%rax, %rcx), %rax
    and %r10, %rax               # pos = (head + count) & mask
    movq 16(%rbx), %r11
    lea (%r11, %rax, 8), %rax
    mov %r14, (%rax)
    movq (%rbx), %rax
    inc %rax
    movq %rax, (%rbx)
    pop %r14
    pop %r12
    pop %rbx
    ret
    .size flint_queue_push, .-flint_queue_push

    .globl flint_queue_pop
    .type flint_queue_pop, @function
# flint_queue_pop(q) -> rax (front value; 0 when empty)
flint_queue_pop:
    push %rbx
    push %r12
    mov %rdi, %rbx
    movq (%rbx), %r10
    test %r10, %r10
    jz .qpe
    movq 8(%rbx), %r11
    dec %r11                     # mask
    movq 24(%rbx), %rcx
    and %r11, %rcx               # pos = head & mask
    movq 16(%rbx), %r12
    lea (%r12, %rcx, 8), %r12
    movq (%r12), %rax
    inc %rcx
    and %r11, %rcx
    movq %rcx, 24(%rbx)
    dec %r10
    movq %r10, (%rbx)
    pop %r12
    pop %rbx
    ret
.qpe:
    xor %rax, %rax
    pop %r12
    pop %rbx
    ret
    .size flint_queue_pop, .-flint_queue_pop

    .globl flint_queue_peek
    .type flint_queue_peek, @function
# flint_queue_peek(q) -> rax (front value; 0 when empty)
flint_queue_peek:
    push %rbx
    push %r12
    mov %rdi, %rbx
    movq (%rbx), %r10
    test %r10, %r10
    jz .qke
    movq 8(%rbx), %r11
    dec %r11
    movq 24(%rbx), %rcx
    and %r11, %rcx
    movq 16(%rbx), %r12
    lea (%r12, %rcx, 8), %r12
    movq (%r12), %rax
    pop %r12
    pop %rbx
    ret
.qke:
    xor %rax, %rax
    pop %r12
    pop %rbx
    ret
    .size flint_queue_peek, .-flint_queue_peek

    .globl flint_queue_size
    .type flint_queue_size, @function
flint_queue_size:
    movq (%rdi), %rax
    ret
    .size flint_queue_size, .-flint_queue_size

    .globl flint_queue_clear
    .type flint_queue_clear, @function
flint_queue_clear:
    movq $0, (%rdi)
    movq $0, 24(%rdi)
    ret
    .size flint_queue_clear, .-flint_queue_clear

# ------------------------------------------------------------------- hashset
    .globl flint_hashset_new
    .type flint_hashset_new, @function
# flint_hashset_new(n) -> rax. Bucket count is the next power of two >= max(8, 2n).
flint_hashset_new:
    push %r12
    push %r14
    mov %rdi, %r10
    shl $1, %r10
    cmp $8, %r10
    jbe .hn_pow
.hn_pow:
    mov $8, %rax
.hn_l:
    cmp %r10, %rax
    jae .hn_d
    shl $1, %rax
    jmp .hn_l
.hn_d:
    mov %rax, %r14               # cap
    mov %r14, %rdi
    shl $4, %rdi
    call flint_alloc               # buckets (16 bytes each)
    mov %rax, %r12
    mov $32, %rdi
    call flint_alloc               # header
    xor %ecx, %ecx
    movq %rcx, (%rax)            # count = 0
    movq %r14, 8(%rax)           # cap
    movq %r12, 16(%rax)          # buckets
    pop %r14
    pop %r12
    ret
    .size flint_hashset_new, .-flint_hashset_new

# internal: double the bucket array and reinsert every live entry
flint_hashset_rehash:
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx
    movq 8(%rbx), %r15           # old cap
    movq 8(%rbx), %r10
    shl $1, %r10                 # new cap
    movq %r10, 8(%rbx)
    mov %r10, %rdi
    shl $4, %rdi
    call flint_alloc
    mov %rax, %r12               # new buckets
    movq 16(%rbx), %r13          # old buckets
    xor %r8, %r8                 # i
.hsr_i:
    cmpq %r15, %r8
    jae .hsr_done
    lea (%r8, %r8, 1), %rdx      # 2i
    lea (%r13, %rdx, 8), %r14    # old bucket
    movq 8(%r14), %r9
    test %r9, %r9
    jz .hsr_next
    cmpq $3, %r9
    je .hsr_next
    movq (%r14), %r11            # value (r11: caller-saved, free)
    mov %r11, %rdi
    mov %r9, %rsi
    call flint_hash_val
    movq 8(%rbx), %r10
    dec %r10
    and %r10, %rax
.hsr_p:
    lea (%rax, %rax, 1), %rdx    # 2h
    lea (%r12, %rdx, 8), %r10    # new bucket
    movq 8(%r10), %rdx
    test %rdx, %rdx
    jz .hsr_ins
    inc %rax
    movq 8(%rbx), %r10
    dec %r10
    and %r10, %rax
    jmp .hsr_p
.hsr_ins:
    movq %r11, (%r10)
    movq %r9, 8(%r10)
.hsr_next:
    inc %r8
    jmp .hsr_i
.hsr_done:
    movq %r12, 16(%rbx)
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret

    .globl flint_hashset_add
    .type flint_hashset_add, @function
# flint_hashset_add(set, value, flag) -> 1 inserted, 0 already present
flint_hashset_add:
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx               # set
    mov %rsi, %r14               # value
    mov %rdx, %r13               # flag
    movq 8(%rbx), %r10
    shl $1, %r10
    cmp %r10, (%rbx)             # count*2 >= cap?
    jb .ha_probe
    mov %rbx, %rdi
    call flint_hashset_rehash
.ha_probe:
    mov %r14, %rdi
    mov %r13, %rsi
    call flint_hash_val
    movq 8(%rbx), %r11
    dec %r11                     # mask
    and %r11, %rax
    movq 16(%rbx), %r12          # buckets
.ha_p:
    lea (%rax, %rax, 1), %rdx    # 2h
    lea (%r12, %rdx, 8), %r10    # bucket
    movq 8(%r10), %r8
    cmpq $3, %r8
    je .ha_next                  # tombstone: keep probing
    test %r8, %r8
    jz .ha_insert
    cmpq %r13, %r8
    je .ha_cand
    jmp .ha_next
.ha_cand:
    cmpq $2, %r13
    je .ha_cands
    cmpq %r14, (%r10)
    je .ha_dup
    jmp .ha_next
.ha_cands:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .ha_dup
    mov %r8, %rax                # restore h
    jmp .ha_next
.ha_next:
    inc %rax
    movq 8(%rbx), %r11
    dec %r11
    and %r11, %rax
    jmp .ha_p
.ha_insert:
    movq %r14, (%r10)
    movq %r13, 8(%r10)
    movq (%rbx), %rax
    inc %rax
    movq %rax, (%rbx)
    mov $1, %rax
    jmp .ha_done
.ha_dup:
    xor %rax, %rax
.ha_done:
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashset_add, .-flint_hashset_add

    .globl flint_hashset_contains
    .type flint_hashset_contains, @function
# flint_hashset_contains(set, value, flag) -> 1/0
flint_hashset_contains:
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx
    mov %rsi, %r14
    mov %rdx, %r13
    movq 8(%rbx), %r11
    dec %r11                     # mask
    mov %r14, %rdi
    mov %rdx, %rsi
    call flint_hash_val
    and %r11, %rax
    movq 16(%rbx), %r12
.hc_p:
    lea (%rax, %rax, 1), %rdx
    lea (%r12, %rdx, 8), %r10
    movq 8(%r10), %r8
    test %r8, %r8
    jz .hc_no
    cmpq $3, %r8
    je .hc_next
    cmpq %r13, %r8
    jne .hc_next
    cmpq $2, %r13
    je .hc_s
    cmpq %r14, (%r10)
    je .hc_yes
    jmp .hc_next
.hc_s:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hc_yes
    mov %r8, %rax                # restore h
    jmp .hc_next
.hc_next:
    inc %rax
    and %r11, %rax
    jmp .hc_p
.hc_yes:
    mov $1, %rax
    jmp .hc_done
.hc_no:
    xor %rax, %rax
.hc_done:
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashset_contains, .-flint_hashset_contains

    .globl flint_hashset_remove
    .type flint_hashset_remove, @function
# flint_hashset_remove(set, value, flag) -> 1 removed, 0 absent (tombstones)
flint_hashset_remove:
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx
    mov %rsi, %r14
    mov %rdx, %r13
    movq 8(%rbx), %r11
    dec %r11
    mov %r14, %rdi
    mov %rdx, %rsi
    call flint_hash_val
    and %r11, %rax
    movq 16(%rbx), %r12
.hsm_p:
    lea (%rax, %rax, 1), %rdx
    lea (%r12, %rdx, 8), %r10
    movq 8(%r10), %r8
    test %r8, %r8
    jz .hsm_no
    cmpq $3, %r8
    je .hsm_next
    cmpq %r13, %r8
    jne .hsm_next
    cmpq $2, %r13
    je .hsm_s
    cmpq %r14, (%r10)
    je .hsm_hit
    jmp .hsm_next
.hsm_s:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hsm_hit
    mov %r8, %rax                # restore h
    jmp .hsm_next
.hsm_next:
    inc %rax
    and %r11, %rax
    jmp .hsm_p
.hsm_hit:
    movq $3, 8(%r10)             # tombstone
    movq (%rbx), %rax
    dec %rax
    movq %rax, (%rbx)
    mov $1, %rax
    jmp .hsm_done
.hsm_no:
    xor %rax, %rax
.hsm_done:
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashset_remove, .-flint_hashset_remove

    .globl flint_hashset_size
    .type flint_hashset_size, @function
flint_hashset_size:
    movq (%rdi), %rax
    ret
    .size flint_hashset_size, .-flint_hashset_size

    .globl flint_hashset_clear
    .type flint_hashset_clear, @function
flint_hashset_clear:
    push %rbx
    mov %rdi, %rbx
    movq $0, (%rbx)
    movq 8(%rbx), %r10
    movq 16(%rbx), %r11
    xor %rcx, %rcx
.hcl_l:
    cmpq %r10, %rcx
    jae .hcl_d
    lea (%rcx, %rcx, 1), %rdx
    lea (%r11, %rdx, 8), %rax
    movq $0, (%rax)
    movq $0, 8(%rax)
    inc %rcx
    jmp .hcl_l
.hcl_d:
    pop %rbx
    ret
    .size flint_hashset_clear, .-flint_hashset_clear

# ------------------------------------------------------------------- hashmap
    .globl flint_hashmap_new
    .type flint_hashmap_new, @function
# flint_hashmap_new(n) -> rax. Slot count is the next power of two >= max(8, 2n).
flint_hashmap_new:
    push %r12
    push %r14
    mov %rdi, %r10
    shl $1, %r10
    cmp $8, %r10
    jbe .mn_pow
.mn_pow:
    mov $8, %rax
.mn_l:
    cmp %r10, %rax
    jae .mn_d
    shl $1, %rax
    jmp .mn_l
.mn_d:
    mov %rax, %r14               # cap
    mov %r14, %rdi
    shl $5, %rdi
    call flint_alloc               # entries (32 bytes each)
    mov %rax, %r12
    mov $32, %rdi
    call flint_alloc               # header
    xor %ecx, %ecx
    movq %rcx, (%rax)            # count = 0
    movq %r14, 8(%rax)           # cap
    movq %r12, 16(%rax)          # entries
    pop %r14
    pop %r12
    ret
    .size flint_hashmap_new, .-flint_hashmap_new

# internal: double the entry array and reinsert every live entry
flint_hashmap_rehash:
    push %rbx
    push %r11
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx
    movq 8(%rbx), %r15           # old cap (r15 survives flint_alloc, which clobbers r11)
    movq 8(%rbx), %r10
    shl $1, %r10                 # new cap
    movq %r10, 8(%rbx)
    mov %r10, %rdi
    shl $5, %rdi
    call flint_alloc
    mov %rax, %r12               # new entries
    movq 16(%rbx), %r13          # old entries
    xor %r8, %r8                 # i
.hmr_i:
    cmpq %r15, %r8
    jae .hmr_done
    mov %r8, %rdx
    shl $2, %rdx                 # 4i
    lea (%r13, %rdx, 8), %r14    # old entry
    movq 8(%r14), %r9
    test %r9, %r9
    jz .hmr_next
    cmpq $3, %r9
    je .hmr_next
    movq (%r14), %rdi            # key
    mov %r9, %rsi
    mov %r8, %r10                # save loop counter (flint_hash_val clobbers r8)
    call flint_hash_val
    mov %r10, %r8                # restore loop counter
    movq 8(%rbx), %r10
    dec %r10
    and %r10, %rax
.hmr_p:
    mov %rax, %rdx
    shl $2, %rdx                 # 4h
    lea (%r12, %rdx, 8), %r10    # new entry
    movq 8(%r10), %rdx
    test %rdx, %rdx
    jz .hmr_ins
    inc %rax
    movq 8(%rbx), %r10
    dec %r10
    and %r10, %rax
    jmp .hmr_p
.hmr_ins:
    mov %r8, %rdx
    shl $2, %rdx                 # 4i
    lea (%r13, %rdx, 8), %r14    # old entry again
    movq (%r14), %rdx
    movq %rdx, (%r10)
    movq %r9, 8(%r10)
    movq 16(%r14), %rdx
    movq %rdx, 16(%r10)
    movq 24(%r14), %rdx
    movq %rdx, 24(%r10)
.hmr_next:
    inc %r8
    jmp .hmr_i
.hmr_done:
    movq %r12, 16(%rbx)
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %r11
    pop %rbx
    ret

    .globl flint_hashmap_put
    .type flint_hashmap_put, @function
# flint_hashmap_put(map, key, kflag, value, vflag) -> 1 inserted, 0 updated
flint_hashmap_put:
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx               # map
    mov %rsi, %r12               # key
    mov %rdx, %r13               # kflag
    mov %rcx, %r14               # value
    mov %r8, %r15                # vflag
    movq (%rbx), %r10
    shl $1, %r10                 # r10 = count * 2
    cmp 8(%rbx), %r10            # count*2 >= cap?
    jb .hp_probe
    mov %rbx, %rdi
    call flint_hashmap_rehash
.hp_probe:
    mov %r12, %rdi
    mov %r13, %rsi
    call flint_hash_val
    movq 8(%rbx), %r10
    dec %r10                     # mask
    and %r10, %rax
    movq 16(%rbx), %r11          # entries
.hp_p:
    mov %rax, %rdx
    shl $2, %rdx                 # 4h
    lea (%r11, %rdx, 8), %r10    # entry
    movq 8(%r10), %r8
    cmpq $3, %r8
    je .hp_next
    test %r8, %r8
    jz .hp_new
    cmpq %r13, %r8
    jne .hp_next
    cmpq $2, %r13
    je .hp_s
    cmpq %r12, (%r10)
    je .hp_upd
    jmp .hp_next
.hp_s:
    movq (%r10), %rdi
    mov %r12, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hp_upd
    mov %r8, %rax                # restore h
    jmp .hp_next
.hp_next:
    inc %rax
    movq 8(%rbx), %r10
    dec %r10
    and %r10, %rax
    jmp .hp_p
.hp_new:
    movq %r12, (%r10)
    movq %r13, 8(%r10)
    movq %r14, 16(%r10)
    movq %r15, 24(%r10)
    movq (%rbx), %rax
    inc %rax
    movq %rax, (%rbx)
    mov $1, %rax
    jmp .hp_done
.hp_upd:
    movq %r14, 16(%r10)
    movq %r15, 24(%r10)
    xor %rax, %rax
.hp_done:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashmap_put, .-flint_hashmap_put

    .globl flint_hashmap_get
    .type flint_hashmap_get, @function
# flint_hashmap_get(map, key, kflag, want_str) -> rax value, or 0 when absent
# or when the stored value kind does not match want_str (0 int, 1 string).
flint_hashmap_get:
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx
    mov %rsi, %r14               # key
    mov %rdx, %r13               # kflag
    mov %r8, %r15                # want_str
    movq 8(%rbx), %r11
    dec %r11
    mov %r14, %rdi
    mov %rdx, %rsi
    call flint_hash_val
    and %r11, %rax
    movq 16(%rbx), %r12
.hg_p:
    mov %rax, %rdx
    shl $2, %rdx
    lea (%r12, %rdx, 8), %r10
    movq 8(%r10), %r8
    test %r8, %r8
    jz .hg_no
    cmpq $3, %r8
    je .hg_next
    cmpq %r13, %r8
    jne .hg_next
    cmpq $2, %r13
    je .hg_s
    cmpq %r14, (%r10)
    je .hg_hit
    jmp .hg_next
.hg_s:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hg_hit
    mov %r8, %rax                # restore h
    jmp .hg_next
.hg_next:
    inc %rax
    and %r11, %rax
    jmp .hg_p
.hg_hit:
    movq 16(%r10), %rax          # value
    lea 1(%r15), %r8             # expected vflag = want_str + 1
    cmpq %r8, 24(%r10)
    je .hg_done
    xor %rax, %rax
    jmp .hg_done
.hg_no:
    xor %rax, %rax
.hg_done:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashmap_get, .-flint_hashmap_get

    .globl flint_hashmap_contains
    .type flint_hashmap_contains, @function
# flint_hashmap_contains(map, key, kflag) -> 1/0
flint_hashmap_contains:
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx
    mov %rsi, %r14
    mov %rdx, %r13
    movq 8(%rbx), %r11
    dec %r11
    mov %r14, %rdi
    mov %rdx, %rsi
    call flint_hash_val
    and %r11, %rax
    movq 16(%rbx), %r12
.hmc_p:
    mov %rax, %rdx
    shl $2, %rdx
    lea (%r12, %rdx, 8), %r10
    movq 8(%r10), %r8
    test %r8, %r8
    jz .hmc_no
    cmpq $3, %r8
    je .hmc_next
    cmpq %r13, %r8
    jne .hmc_next
    cmpq $2, %r13
    je .hmc_s
    cmpq %r14, (%r10)
    je .hmc_yes
    jmp .hmc_next
.hmc_s:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hmc_yes
    mov %r8, %rax                # restore h
    jmp .hmc_next
.hmc_next:
    inc %rax
    and %r11, %rax
    jmp .hmc_p
.hmc_yes:
    mov $1, %rax
    jmp .hmc_done
.hmc_no:
    xor %rax, %rax
.hmc_done:
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashmap_contains, .-flint_hashmap_contains

    .globl flint_hashmap_remove
    .type flint_hashmap_remove, @function
# flint_hashmap_remove(map, key, kflag) -> 1 removed, 0 absent (tombstones)
flint_hashmap_remove:
    push %rbx
    push %r12
    push %r13
    push %r14
    mov %rdi, %rbx
    mov %rsi, %r14
    mov %rdx, %r13
    movq 8(%rbx), %r11
    dec %r11
    mov %r14, %rdi
    mov %rdx, %rsi
    call flint_hash_val
    and %r11, %rax
    movq 16(%rbx), %r12
.hmr2_p:
    mov %rax, %rdx
    shl $2, %rdx
    lea (%r12, %rdx, 8), %r10
    movq 8(%r10), %r8
    test %r8, %r8
    jz .hmr2_no
    cmpq $3, %r8
    je .hmr2_next
    cmpq %r13, %r8
    jne .hmr2_next
    cmpq $2, %r13
    je .hmr2_s
    cmpq %r14, (%r10)
    je .hmr2_hit
    jmp .hmr2_next
.hmr2_s:
    movq (%r10), %rdi
    mov %r14, %rsi
    mov %rax, %r8                # save probe pos h
    call flint_strcmp
    test %rax, %rax
    jz .hmr2_hit
    mov %r8, %rax                # restore h
    jmp .hmr2_next
.hmr2_next:
    inc %rax
    and %r11, %rax
    jmp .hmr2_p
.hmr2_hit:
    movq $3, 8(%r10)
    movq (%rbx), %rax
    dec %rax
    movq %rax, (%rbx)
    mov $1, %rax
    jmp .hmr2_done
.hmr2_no:
    xor %rax, %rax
.hmr2_done:
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashmap_remove, .-flint_hashmap_remove

    .globl flint_hashmap_size
    .type flint_hashmap_size, @function
flint_hashmap_size:
    movq (%rdi), %rax
    ret
    .size flint_hashmap_size, .-flint_hashmap_size

    .globl flint_hashmap_clear
    .type flint_hashmap_clear, @function
flint_hashmap_clear:
    push %rbx
    mov %rdi, %rbx
    movq $0, (%rbx)
    movq 8(%rbx), %r10
    movq 16(%rbx), %r11
    xor %rcx, %rcx
.hmcl_l:
    cmpq %r10, %rcx
    jae .hmcl_d
    mov %rcx, %rdx
    shl $2, %rdx
    lea (%r11, %rdx, 8), %rax
    movq $0, (%rax)
    movq $0, 8(%rax)
    movq $0, 16(%rax)
    movq $0, 24(%rax)
    inc %rcx
    jmp .hmcl_l
.hmcl_d:
    pop %rbx
    ret
    .size flint_hashmap_clear, .-flint_hashmap_clear

    .globl flint_hashmap_keys
    .type flint_hashmap_keys, @function
# flint_hashmap_keys(map) -> rax: a list of the stored keys (insertion order)
flint_hashmap_keys:
    push %rbx
    push %r12
    push %r13
    mov %rdi, %rbx
    movq (%rbx), %rdi
    call flint_list_new            # rax = list with room for count items
    mov %rax, %r12
    movq 16(%rbx), %r13          # entries
    movq 8(%rbx), %r11
    xor %r8, %r8                 # i
    xor %rcx, %rcx               # out index
.hk_i:
    cmpq %r11, %r8
    jae .hk_done
    mov %r8, %rdx
    shl $2, %rdx
    lea (%r13, %rdx, 8), %r10
    movq 8(%r10), %r9
    test %r9, %r9
    jz .hk_next
    cmpq $3, %r9
    je .hk_next
    movq 16(%r12), %r11
    lea (%r11, %rcx, 8), %r11
    movq (%r10), %rdx
    movq %rdx, (%r11)            # key value
    inc %rcx
    movq 8(%rbx), %r11           # restore slot count
.hk_next:
    inc %r8
    jmp .hk_i
.hk_done:
    movq %rcx, (%r12)            # list len
    pop %r13
    pop %r12
    pop %rbx
    ret
    .size flint_hashmap_keys, .-flint_hashmap_keys

    .globl flint_hashmap_values
    .type flint_hashmap_values, @function
# flint_hashmap_values(map) -> rax: a list of the stored values (insertion order)
flint_hashmap_values:
    push %rbx
    push %r12
    push %r13
    mov %rdi, %rbx
    movq (%rbx), %rdi
    call flint_list_new
    mov %rax, %r12
    movq 16(%rbx), %r13
    movq 8(%rbx), %r11
    xor %r8, %r8
    xor %rcx, %rcx
.hv2_i:
    cmpq %r11, %r8
    jae .hv2_done
    mov %r8, %rdx
    shl $2, %rdx
    lea (%r13, %rdx, 8), %r10
    movq 8(%r10), %r9
    test %r9, %r9
    jz .hv2_next
    cmpq $3, %r9
    je .hv2_next
    movq 16(%r12), %r11
    lea (%r11, %rcx, 8), %r11
    movq 16(%r10), %rdx
    movq %rdx, (%r11)            # value
    inc %rcx
    movq 8(%rbx), %r11
.hv2_next:
    inc %r8
    jmp .hv2_i
.hv2_done:
    movq %rcx, (%r12)
    pop %r13
    pop %r12
    pop %rbx
    ret

    .globl flint_list_sort
    .type flint_list_sort, @function
# flint_list_sort(list, mode): in-place insertion sort. mode 0 = signed int
# ascending; mode 1 = string content ascending (strcmp).
flint_list_sort:
    push %rbx
    push %r8
    push %r9
    push %r11
    push %r12
    push %r14
    mov %rdi, %rbx             # list
    mov %rsi, %r12             # mode
    movq 16(%rbx), %r13        # data
    movq (%rbx), %r14         # len
    xor %r11, %r11            # i = 0
    test %r14, %r14
    jle .ls_done
.ls_outer:
    inc %r11                  # i = 1
    cmp %r14, %r11
    jae .ls_done
    lea -1(%r11), %r8        # j = i - 1
    test %r8, %r8
    js .ls_outer             # j < 0: next i
.ls_inner:
    movq (%r13, %r8, 8), %r9   # a = data[j]
    movq 8(%r13, %r8, 8), %rax # b = data[j+1]
    test %r12, %r12
    jz .ls_int_cmp            # int mode
    mov %rax, %r10            # b (flint_strcmp clobbers rax)
    mov %r9, %rdi
    mov %r10, %rsi
    call flint_strcmp
    test %rax, %rax
    jle .ls_inr_done          # a <= b: no swap
    mov %r10, %rax            # restore b for the swap
    jmp .ls_swap
.ls_int_cmp:
    cmp %rax, %r9              # a - b (signed)
    jg .ls_swap               # a > b: swap
    jmp .ls_inr_done
.ls_swap:
    mov %r9, 8(%r13, %r8, 8)
    mov %rax, (%r13, %r8, 8)
.ls_inr_done:
    dec %r8
    test %r8, %r8
    jns .ls_inner
    jmp .ls_outer
.ls_done:
    pop %r14
    pop %r12
    pop %r11
    pop %r9
    pop %r8
    pop %rbx
    ret
    .size flint_list_sort, .-flint_list_sort

    .globl flint_list_reverse
    .type flint_list_reverse, @function
# flint_list_reverse(list): reverse element order in place.
flint_list_reverse:
    push %rbx
    push %r12
    push %r14
    mov %rdi, %rbx             # list
    movq 16(%rbx), %r12        # data
    movq (%rbx), %r14         # len
    test %r14, %r14
    jle .lrv_done             # len <= 1: nothing to do
    xor %rax, %rax            # i = 0
    lea -1(%r14), %r11        # j = len - 1
    cmp %r11, %rax
    jae .lrv_done             # i >= j: nothing to do
.lrv_loop:
    movq (%r12, %rax, 8), %r10
    movq (%r12, %r11, 8), %r8
    mov %r10, (%r12, %r11, 8)
    mov %r8, (%r12, %rax, 8)
    inc %rax
    dec %r11
    cmp %r11, %rax
    jae .lrv_done
    jmp .lrv_loop
.lrv_done:
    pop %r14
    pop %r12
    pop %rbx
    ret
    .size flint_list_reverse, .-flint_list_reverse

    .globl flint_list_insert
    .type flint_list_insert, @function
# flint_list_insert(list, index, value): insert value at index (shifting the
# tail right); index >= len appends. Grows like flint_list_add when full.
flint_list_insert:
    push %rbx
    push %r8
    push %r9
    push %r10
    push %r11
    push %r12
    push %r14
    mov %rdi, %rbx             # list
    mov %rsi, %r14             # index
    mov %rdx, %r12             # value
    movq (%rbx), %r10         # len
    cmp %r10, %r14            # index - len
    jb .li_idx_ok
    mov %r10, %r14            # index = len (append)
.li_idx_ok:
    movq 8(%rbx), %r11        # cap
    cmp %r10, %r11            # cap - len
    ja .li_shift              # cap > len: room for len+1, no growth
    shl $1, %r11
    test %r11, %r11
    jnz .li_grow
    mov $8, %r11
.li_grow:
    mov %r11, %r8             # new cap
    mov %r8, %rdi
    shl $3, %rdi
    call flint_alloc            # rax = new buffer
    movq 16(%rbx), %rsi       # old data
    movq %r10, %rdx
    shl $3, %rdx              # len * 8
    call flint_memcpy           # rdi = new + len*8
    sub %rdx, %rdi
    movq %rdi, 16(%rbx)       # data = new
    movq %r8, 8(%rbx)         # cap = new cap
.li_shift:
    # shift data[len-1 .. index] one slot right
    movq 16(%rbx), %rax       # data
    lea (%rax, %r10, 8), %r8  # dst = data + len*8
    cmp %r10, %r14            # index - len
    jae .li_store             # index == len: nothing to shift
    lea -8(%r8), %r11        # src = data + (len-1)*8
    lea 8(%rax, %r14, 8), %r9 # stop = data + (index+1)*8
.li_sh:
    movq (%r11), %rax
    mov %rax, (%r8)
    sub $8, %r8
    sub $8, %r11
    cmp %r9, %r8
    jae .li_sh
.li_store:
    movq 16(%rbx), %rax       # data
    mov %r12, (%rax, %r14, 8) # data[index] = value
    incq (%rbx)               # len++
    pop %r14
    pop %r12
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rbx
    ret
    .size flint_list_insert, .-flint_list_insert

    .globl flint_list_join
    .type flint_list_join, @function
# flint_list_join(list, sep, mode) -> rax: a fresh NUL-terminated string of
# the elements joined by sep. mode 0: elements are ints (decimal via
# flint_itoa); mode 1: elements are strings.
flint_list_join:
    push %rbx
    push %r8
    push %r9
    push %r10
    push %r11
    push %r12
    push %r13
    push %r14
    sub $8, %rsp               # [rsp] = loop counter slot (flint_memcpy
                              # clobbers every general register we hold)
    mov %rdi, %rbx             # list
    mov %rsi, %r12             # sep
    mov %rdx, %r14             # mode
    movq 16(%rbx), %r13        # data
    movq (%rbx), %r8           # len
    test %r8, %r8
    jz .lj_zero
    # phase 1: total element bytes
    xor %r9, %r9
    lea -1(%r8), %r11
.lj_len:
    movq (%r13, %r11, 8), %r15
    test %r14, %r14
    jnz .lj_slen
    mov %r15, %rdi
    call flint_itoa
    mov %rax, %r15
.lj_slen:
    mov %r15, %rdi
    call flint_strlen
    add %rax, %r9
    dec %r11
    test %r11, %r11
    jns .lj_len
    # + seplen * (len - 1); flint_alloc (via flint_itoa) clobbered r8
    mov (%rbx), %r8
    mov %r12, %rdi
    call flint_strlen
    mov %rax, %rdi
    lea -1(%r8), %rsi
    mul %rsi
    add %rax, %r9
    # phase 2: fill
    lea 1(%r9), %rdi
    call flint_alloc
    mov %rax, %r13             # buf
    mov (%rbx), %r8           # len (flint_alloc clobbered r8)
    mov %r13, %r9             # cursor
    movq 16(%rbx), %r11        # data
    xor %r10, %r10            # i = 0 (ascending)
    mov %r10, (%rsp)
.lj_fill:
    mov (%rsp), %r10
    movq (%r11, %r10, 8), %r15
    test %r14, %r14
    jnz .lj_fstr
    mov %r15, %rdi
    call flint_itoa
    mov %rax, %r15
    mov (%rbx), %r8           # flint_itoa's flint_alloc clobbered r8
.lj_fstr:
    # r15 = element string; first element gets no separator
    test %r10, %r10
    jz .lj_fcopy
    # flint_memcpy clobbers r10, so the separator length is recomputed here
    mov %r12, %rdi
    call flint_strlen
    mov %rax, %rdx
    mov %r12, %rsi
    mov %r9, %rdi
    call flint_memcpy           # rdi = cursor + seplen
    mov %rdi, %r9
.lj_fcopy:
    mov %r15, %rdi
    call flint_strlen
    mov %rax, %rdx
    mov %r15, %rsi
    mov %r9, %rdi
    call flint_memcpy           # rdi = cursor + elem len
    mov %rdi, %r9
    mov (%rsp), %r10         # memcpy clobbered r10
    inc %r10
    mov %r10, (%rsp)
    cmp %r8, %r10             # r8 = len
    jne .lj_fill
    movb $0, (%r9)            # NUL terminator
    mov %r13, %rax
    add $8, %rsp
    pop %r14
    pop %r13
    pop %r12
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rbx
    ret
.lj_zero:
    lea 16, %rdi
    call flint_alloc
    movb $0, (%rax)
    add $8, %rsp
    pop %r14
    pop %r13
    pop %r12
    pop %r11
    pop %r10
    pop %r9
    pop %r8
    pop %rbx
    ret
    .size flint_list_join, .-flint_list_join
