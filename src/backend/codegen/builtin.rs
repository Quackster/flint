use crate::ast::Ty;

/// A known builtin: (asm target, arity, result type).
pub(crate) struct Builtin {
    pub target: &'static str,
    pub arity: usize,
    pub ret: Ty,
    pub noreturn: bool,
}

pub(crate) fn builtin_for(path: &[String]) -> Option<Builtin> {
    let name = path.join(".");
    match name.as_str() {
        "sys.exit" | "exit" => Some(Builtin {
            target: "flint_exit",
            arity: 1,
            ret: Ty::Void,
            noreturn: true,
        }),
        "sys.write" | "write" => Some(Builtin {
            target: "flint_write",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.read" | "read" => Some(Builtin {
            target: "flint_read",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.open" | "open" => Some(Builtin {
            target: "flint_open",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.close" | "close" => Some(Builtin {
            target: "flint_close",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.brk" | "brk" => Some(Builtin {
            target: "flint_brk",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "sys.socket" | "socket" => Some(Builtin {
            target: "flint_socket",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.bind" | "bind" => Some(Builtin {
            target: "flint_bind",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.listen" | "listen" => Some(Builtin {
            target: "flint_listen",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.accept" | "accept" => Some(Builtin {
            target: "flint_accept",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.connect" | "connect" => Some(Builtin {
            target: "flint_connect",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.sockaddr" | "sockaddr" => Some(Builtin {
            target: "flint_sockaddr",
            arity: 5,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "sys.alloc" | "alloc" | "mem.alloc" => Some(Builtin {
            target: "flint_alloc",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "sys.free" | "free" | "mem.free" => Some(Builtin {
            target: "flint_free",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        // Reference counting for class objects: bump / drop the refcount.
        "mem.retain" => Some(Builtin {
            target: "flint_retain",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "mem.release" => Some(Builtin {
            target: "flint_release",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "memcpy" | "mem.memcpy" => Some(Builtin {
            target: "flint_memcpy",
            arity: 3,
            ret: Ty::Void,
            noreturn: false,
        }),
        "strlen" | "str.len" => Some(Builtin {
            target: "flint_strlen",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        // Polymorphic: an int (or bool/enum) argument prints as decimal,
        // anything else as a string; the backend picks the variant by the
        // argument type (see gen_call).
        "print" | "io.print" => Some(Builtin {
            target: "flint_print",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "println" | "io.println" => Some(Builtin {
            target: "flint_println",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "atoi" | "conv.atoi" => Some(Builtin {
            target: "flint_atoi",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        // array length (slot 0 of the array header)
        "len" => Some(Builtin {
            target: "flint_len",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "str.cmp" | "strcmp" => Some(Builtin {
            target: "flint_strcmp",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "str.copy" | "strcpy" => Some(Builtin {
            target: "flint_strcopy",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "str.concat" | "concat" => Some(Builtin {
            target: "flint_strconcat",
            arity: 2,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "str.concati" | "concati" => Some(Builtin {
            target: "flint_strconcati",
            arity: 2,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "str.itoa" | "itoa" => Some(Builtin {
            target: "flint_itoa",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "time.millis" | "time.now" | "time" => Some(Builtin {
            target: "flint_time_millis",
            arity: 0,
            ret: Ty::Int,
            noreturn: false,
        }),
        "rand.next" | "rand" => Some(Builtin {
            target: "flint_rand_next",
            arity: 0,
            ret: Ty::Int,
            noreturn: false,
        }),
        "rand.range" => Some(Builtin {
            target: "flint_rand_range",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "env.get" | "getenv" => Some(Builtin {
            target: "flint_env_get",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "str.substring" => Some(Builtin {
            target: "flint_str_substring",
            arity: 3,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "str.indexOf" => Some(Builtin {
            target: "flint_str_indexof",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "str.replace" => Some(Builtin {
            target: "flint_str_replace",
            arity: 3,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "b64.encode" => Some(Builtin {
            target: "flint_b64_encode",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "b64.decode" => Some(Builtin {
            target: "flint_b64_decode",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "log.info" => Some(Builtin {
            target: "flint_log_info",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "log.warn" => Some(Builtin {
            target: "flint_log_warn",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "log.error" => Some(Builtin {
            target: "flint_log_error",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "log.debug" => Some(Builtin {
            target: "flint_log_debug",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "sys.select" => Some(Builtin {
            target: "flint_select",
            arity: 5,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.poll" => Some(Builtin {
            target: "flint_poll",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.epollCreate1" => Some(Builtin {
            target: "flint_epoll_create1",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.epollCtl" => Some(Builtin {
            target: "flint_epoll_ctl",
            arity: 4,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.epollWait" => Some(Builtin {
            target: "flint_epoll_wait",
            arity: 4,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.clone" => Some(Builtin {
            target: "flint_clone",
            arity: 5,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.futex" => Some(Builtin {
            target: "flint_futex",
            arity: 6,
            ret: Ty::Int,
            noreturn: false,
        }),
        // Generic syscall escape hatch: sys.syscall(num, a1, a2, a3, a4, a5).
        // The 4th syscall argument arrives in %rcx (System V call order) and
        // is moved to %r10 by the intrinsic, per the syscall ABI.
        "sys.syscall" => Some(Builtin {
            target: "flint_syscall",
            arity: 6,
            ret: Ty::Int,
            noreturn: false,
        }),
        // Memory mapping: mmap(addr, len, prot, flags, fd, offset) -> ptr.
        // PROT_READ|PROT_WRITE = 3; MAP_SHARED = 1, MAP_PRIVATE|MAP_ANON = 0x22.
        "sys.mmap" => Some(Builtin {
            target: "flint_mmap",
            arity: 6,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "sys.munmap" => Some(Builtin {
            target: "flint_munmap",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.byteLoad" | "byteLoad" => Some(Builtin {
            target: "flint_byte_load",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.byteStore" | "byteStore" => Some(Builtin {
            target: "flint_byte_store",
            arity: 2,
            ret: Ty::Void,
            noreturn: false,
        }),
        "sys.shortLoad" | "shortLoad" => Some(Builtin {
            target: "flint_short_load",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.shortStore" | "shortStore" => Some(Builtin {
            target: "flint_short_store",
            arity: 2,
            ret: Ty::Void,
            noreturn: false,
        }),
        "sys.intLoad" | "intLoad" => Some(Builtin {
            target: "flint_int_load",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.intStore" | "intStore" => Some(Builtin {
            target: "flint_int_store",
            arity: 2,
            ret: Ty::Void,
            noreturn: false,
        }),
        // Fixed-point float (Q48.16): 64-bit int, lower 16 bits = fractional.
        "math.fadd" | "fadd" => Some(Builtin {
            target: "flint_fadd",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.fsub" | "fsub" => Some(Builtin {
            target: "flint_fsub",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.fmul" | "fmul" => Some(Builtin {
            target: "flint_fmul",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.fdiv" | "fdiv" => Some(Builtin {
            target: "flint_fdiv",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.ftoi" | "ftoi" => Some(Builtin {
            target: "flint_ftoi",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.itof" | "itof" => Some(Builtin {
            target: "flint_itof",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "math.fcmp" | "fcmp" => Some(Builtin {
            target: "flint_fcmp",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        // Thread and synchronization builtins.
        "sys.threadCreate" => Some(Builtin {
            target: "flint_thread_create",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.threadJoin" => Some(Builtin {
            target: "flint_thread_join",
            arity: 1,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.mutexLock" => Some(Builtin {
            target: "flint_mutex_lock",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "sys.mutexUnlock" => Some(Builtin {
            target: "flint_mutex_unlock",
            arity: 1,
            ret: Ty::Void,
            noreturn: false,
        }),
        "sys.atomicCas" => Some(Builtin {
            target: "flint_atomic_cas",
            arity: 3,
            ret: Ty::Int,
            noreturn: false,
        }),
        "sys.nanosleep" => Some(Builtin {
            target: "flint_nanosleep",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        // JSON key-value extraction.
        "json.get" => Some(Builtin {
            target: "flint_json_get",
            arity: 2,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        "json.geti" => Some(Builtin {
            target: "flint_json_geti",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        // Varargs-style string formatting: str.format(fmt, a, b, c, d) -> *byte
        // Supports %d (int), %s (*byte), %b (int as bool) placeholders.
        "str.format" => Some(Builtin {
            target: "flint_str_format",
            arity: 5,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        // Retain a class value regardless of its static type (lambda
        // captures); null-guarded. Returns the value.
        "mem.retainVal" => Some(Builtin {
            target: "flint_retain_val",
            arity: 1,
            ret: Ty::Ptr(None),
            noreturn: false,
        }),
        // Call a closure: sys.fnCall2(block, a) invokes the lifted function
        // as fn(ctx = block, a).
        "sys.fnCall2" | "fnCall2" => Some(Builtin {
            target: "flint_fn_call2",
            arity: 2,
            ret: Ty::Int,
            noreturn: false,
        }),
        _ => None,
    }
}
