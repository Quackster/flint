# AGENTS.md — contributing to flint / flintc

Guidance for agents (and humans) making changes to this repo. The compiler
`flintc` is written in Rust (zero external deps); the target language is a
small, statically-typed, systems-level language that compiles to freestanding
x86_64 Linux assembly (no libc).

## Build & test

```sh
cargo build                 # -> target/debug/flintc (release: --release)
bash tests/run_tests.sh     # full suite (expect: PASS=95  FAIL=0)
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
- Trailing parameters may carry default values
  (`void f(int a, int b = 0)`; called as `f(1)` or `f(1, 2)`). Defaults
  must be constants (literals, enum variants, or `Class.staticField`
  references) — no calls or expressions — and are not allowed on
  constructors or lambda parameters.
- Naming: classes `PascalCase` (`Num`, `Thread`); variables and fields
  `snake_case` (`grand_count`).
- **Function naming: every function and method is `camelCase` — including the
  standard functions (builtins such as `sys.byteLoad`, `sys.threadCreate`,
  `str.indexOf`) and all `std` library methods (`Thread.nCpu`,
  `Socket.bindPort`, `Mem.intArray`). Never use `snake_case` for a function
  name** (`nCpu`, not `n_cpu`; `readAll`, not `read_all`).
- One statement per line; keep bodies short.

## Writing examples

Examples (`examples/*.flint`) are the language's public face. They must read
like ordinary application code, **not** like a systems-programming demo:

- **Use the high-level stdlib API** (`src/stdlib/*.flint`). Do **not** call the
  low-level primitives directly in an example:
  - `alloc`, `free`, `memcpy`
  - `sys.syscall`, `sys.read`, `sys.write`, `sys.mmap`, ...
  - raw memory ops: `sys.byteLoad`/`byteStore`, `sys.shortLoad`/`shortStore`,
    `sys.intLoad`/`intStore`
- If a capability an example needs is missing from the stdlib, **add a stdlib
  wrapper for it** (in `package std;`, e.g. `class Thread`) and have the example
  call that wrapper. Keep the raw `sys.*` / `alloc` calls *inside* the stdlib
  module, where they belong. Models: `thread.flint` (Thread: nCpu/spawn/join),
  `net.flint` (Socket: stream/bindPort/bindHost/listen/accept/connectHost/
  sendAll/recv/close/nthDot), `sync.flint` (Sync: lock/unlock/cas/nanosleep),
  `mem.flint`
  (Mem: intArray/bytes/copy/freeInt/freeByte), `file.flint` (File: readAll/writeAll/
  copy/size/...).
- Show the build line in the header comment, including the stdlib files it
  needs (e.g. `flintc src/stdlib/thread.flint examples/...flint -o ...`).
- **Exactly one** example may show raw `alloc` / `free` / `memcpy`:
  `rawmem.flint`, the low-level memory reference. **Every other example must
  allocate through `std.Mem`** (`Mem.intArray` / `Mem.bytes` / `Mem.freeInt` /
  `Mem.freeByte` /
  `Mem.copy`) and use the stdlib (`File`, `Socket`, `Thread`, `Sync`) for any
  other capability. (The byte-level parts of `strings2.flint` also *document*
  the raw string API and may show `sys.byteLoad`, but must not use raw
  `alloc`.)
- Prefer `for` loops and small, focused helpers; use `str.itoa(n)` (or
  `println(n)`) to print numbers — never rely on `string + int`.

## Where things live

- `src/*.rs` — the Rust compiler (lexer, parser, monomorphize, backend/codegen).
- `src/intrinsics.s` — the freestanding runtime (raw syscalls, heap,
  refcounting, threads, sockets, print helpers, the `destroy()` hook).
  There is no separate collection runtime: the collections are ordinary
  generic stdlib classes in `src/stdlib/coll.flint` (`Collection`,
  `List<T>`, `Queue<T>`, `HashSet<T>`, `HashMap<K, V>`). The element
  flavour (0 int, 1 string, 2 object) is baked into the constructor by the
  monomorphizer from the concrete type argument (no manual `kind` field).
- `src/prelude/*.flint` — builtin reference docs (`mem`, `str`, `sys`, `io`,
  `conv`); these illustrate always-available builtins.
- `src/stdlib/*.flint` — the `std` library (each file declares `package std;`).
- `examples/*.flint` — runnable examples (the code's public face — see above).
- `tests/` — the test suite (`run_tests.sh`), plus `tests/cases` for error tests.
- `docs/` — the public documentation site (`index.html` + `app.js`/`style.css`):
  one `<section id="...">` per topic, a `Standard library` nav section with a
  link **per stdlib class** (e.g. `#std-file`), and `concurrency`/`networking`/
  `io` how-to sections that lead with the stdlib.

## Keeping the docs in sync

`docs/` is the public face of the language and **must stay current**:

- **Every time a feature, stdlib class, builtin, or example is added,
  changed, or removed, update `docs/` in the same change.** A new stdlib class
  in `src/stdlib/` needs a `Standard library: <X>` section *and* a nav link in
  the `Standard library` nav section; a changed API, example, or behaviour must
  be reflected in the matching section.
- Keep `docs/index.html` consistent with `README.md`: the same sections, the
  same stdlib class list, and the same examples/build lines. When one changes,
  change both.
- Check the `Standard library` nav section against `src/stdlib/*.flint`: every
  class file should have a section, and every section should match the class
  signature. A missing/extra entry means the docs are stale.
