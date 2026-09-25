# flint / flintc

A small, statically-typed, systems-level language that compiles to
freestanding x86_64 Linux assembly (no libc). The `flintc` compiler is written
in Rust with zero external dependencies.

**Full documentation — language reference, features, examples, and the standard
library — lives at <https://h4bbo.net/flint>.**

- Source: `*.flint`
- Pipeline: `lexer -> parser -> monomorphize -> codegen -> .s -> as -> ld -e _start`
- Runtime: a hand-written freestanding `intrinsics.s` + `collections.s`
  (raw `syscall`s, heap, refcount, threads, sockets, print helpers) linked in
  by the driver.

## Quick start

```sh
git clone https://github.com/Quackster/flint.git
cd flint
cargo build --release          # -> target/release/flintc
cargo build                    # -> target/debug/flintc
```

Requires `as` and `ld` (binutils) on the PATH.

```sh
flintc examples/hello.flint -o hello     # compile -> link -> executable
./hello                               # hello, world

flintc examples/hello.flint -S            # emit x86_64 assembly only (stdout)
flintc examples/hello.flint -o out.s -S   # ... or write the .s to a file
```

Multiple files, a manifest, or a whole directory:

```sh
flintc a.flint b.flint -o out              # concatenate (each keeps its own `package` header)
flintc --manifest box.toml -o out      # read a TOML manifest:  sources = ["util.flint", "main.flint"]
flintc --dir src -o out               # compile every .flint in a directory (sorted)
```

CLI flags: `-o <FILE>` (output, default = first input without `.flint`), `-S`
(assembly only), `--manifest <FILE>`, `--dir <DIR>`, `-h`/`--help`.

Tests: `cargo test` (Rust integration, end-to-end via the flintc binary) and
`tests/run_tests.sh` (execution + golden-asm + spanned-error tests).

## Compiler architecture & layout

Pipeline: `lexer -> parser -> monomorphize -> codegen -> .s -> as -> ld -e _start`.

- **middle/**: monomorphization (`ensure`, `expand`, `infer`, `resolve`,
  `resolve_expr`): generic templates are expanded to concrete types/functions.
- **backend/**: code generation (`codegen/`: `accessor`, `binop`, `builtin`,
  `call`, `coll`, `ctx`, `expr`, `func`, `lvalue`, `new`, `stmt`) plus
  `escape.rs` (stack-vs-heap escape analysis), `layout.rs`, and `release.rs`.
- **intrinsics.s** + **collections.s**: the freestanding runtime, embedded at
  compile time and linked into every binary.

```
Cargo.toml  README.md
src/
  main.rs  ast.rs  lexer.rs  parser.rs  token.rs  span.rs  error.rs
  backend/  mod.rs  codegen.rs  escape.rs  layout.rs  release.rs
    codegen/  accessor.rs  binop.rs  builtin.rs  call.rs  coll.rs
                ctx.rs  expr.rs  func.rs  lvalue.rs  new.rs  stmt.rs
  middle/   mod.rs  mono/  ensure.rs  expand.rs  infer.rs  mod.rs
                resolve.rs  resolve_expr.rs
  prelude/  sys.flint  io.flint  mem.flint  str.flint  conv.flint
   stdlib/   bit.flint  checksum.flint  file.flint  gui.flint
             image.flint  math.flint  num.flint  path.flint
             rand.flint  sort.flint  str.flint  time.flint
             widget.flint
  intrinsics.s  collections.s
tests/  flintc.rs  run_tests.sh  cases/  golden/  errors/  multifile/  thread_*.flint
examples/  hello.flint  fib.flint  file_copy.flint  alloc_demo.flint
            tcp_client.flint  tcp_server.flint  gui.flint  window_gui.flint
            form.flint  windows.flint
```
