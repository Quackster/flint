# AGENTS.md — contributing to flint / flintc

Guidance for agents (and humans) making changes to this repo. The compiler
`flintc` is written in Rust (zero external deps); the target language is a
small, statically-typed, systems-level language that compiles to freestanding
x86_64 Linux assembly (no libc).

## Build & test

```sh
cargo build                 # -> target/debug/flintc (release: --release)
bash tests/run_tests.sh     # full suite (expect: PASS=81  FAIL=0)
```

Requires `as` and `ld` (binutils) on the PATH. Compile one program:

```sh
flintc src/stdlib/thread.flint examples/primes_thread.flint -o primes_thread && ./primes_thread
```

Pass the stdlib/prelude files you need as extra source files (there is no
auto-include); the example's header comment states its exact build line.

## Code style (Java-like)

Follow the Java conventions the language is built around:

- 4-space indentation (no tabs), K&R braces (opening brace on the same line).
- Types spelled out on every declaration: `int n`, `void worker(int k)`,
  `string s`, `*int buf`.
- Control flow in C/Java form: `for (int i = 0; i < n; i = i + 1) { ... }`,
  `while`, `if`/`else`.
- Naming: classes `PascalCase` (`Num`, `Thread`); methods and variables
  `snake_case` (`n_cpu`, `to_hex`, `grand_count`).
- One statement per line; keep bodies short.

## Writing examples

Examples (`examples/*.flint`) are the language's public face. They must read
like ordinary application code, **not** like a systems-programming demo:

- **Use the high-level stdlib API** (`src/stdlib/*.flint`). Do **not** call the
  low-level primitives directly in an example:
  - `alloc`, `free`, `memcpy`
  - `sys.syscall`, `sys.read`, `sys.write`, `sys.mmap`, ...
  - raw memory ops: `sys.byte_load`/`byte_store`, `sys.short_*`, `sys.int_*`
- If a capability an example needs is missing from the stdlib, **add a stdlib
  wrapper for it** (in `package std;`, e.g. `class Thread`) and have the example
  call that wrapper. Keep the raw `sys.*` / `alloc` calls *inside* the stdlib
  module, where they belong. Models: `thread.flint` (Thread: n_cpu/spawn/join),
  `net.flint` (Socket: stream/bind_port/listen/accept/connect_host/send_all/
  recv/close), `sync.flint` (Sync: lock/unlock/cas/nanosleep), `mem.flint`
  (Mem: int_array/bytes/copy/free_*), `file.flint` (File: read_all/write_all/
  copy/size/...).
- Show the build line in the header comment, including the stdlib files it
  needs (e.g. `flintc src/stdlib/thread.flint examples/...flint -o ...`).
- **Exactly one** example may show raw `alloc` / `free` / `memcpy`:
  `rawmem.flint`, the low-level memory reference. **Every other example must
  allocate through `std.Mem`** (`Mem.int_array` / `Mem.bytes` / `Mem.free_*` /
  `Mem.copy`) and use the stdlib (`File`, `Socket`, `Thread`, `Sync`) for any
  other capability. (The byte-level parts of `strings2.flint` also *document*
  the raw string API and may show `sys.byte_load`, but must not use raw
  `alloc`.)
- Prefer `for` loops and small, focused helpers; use `str.itoa(n)` (or
  `printi(n)`) to print numbers — never rely on `string + int`.

## Where things live

- `src/*.rs` — the Rust compiler (lexer, parser, monomorphize, backend/codegen).
- `src/intrinsics.s`, `src/collections.s` — the freestanding runtime (raw
  syscalls, heap, refcounting, threads, sockets, print helpers).
- `src/prelude/*.flint` — builtin reference docs (`mem`, `str`, `sys`, `io`,
  `conv`); these illustrate always-available builtins.
- `src/stdlib/*.flint` — the `std` library (each file declares `package std;`).
- `examples/*.flint` — runnable examples (the code's public face — see above).
- `tests/` — the test suite (`run_tests.sh`), plus `tests/cases` for error tests.
