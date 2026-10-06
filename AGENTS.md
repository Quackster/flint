# AGENTS.md — contributing to flint / flintc

Guidance for agents (and humans) making changes to this repo. The compiler
`flintc` is written in Rust (zero external deps); the target language is a
small, statically-typed, systems-level language that compiles to freestanding
x86_64 Linux assembly (no libc).

## Build & test

```sh
cargo build                 # -> target/debug/flintc (release: --release)
bash tests/run_tests.sh     # full suite (expect: PASS=125  FAIL=0)
```

Requires `as` and `ld` (binutils) on the PATH. Compile one program:

```sh
flintc src/stdlib/thread.flint examples/primes_thread.flint -o primes_thread && ./primes_thread
```

Pass the stdlib/prelude files you need as extra source files (there is no
auto-include); the example's header comment states its exact build line.

## Commits

Commit every milestone once finished: when a feature (or other self-contained
piece of work) is complete and the full test suite passes, make a git commit
for it right away — do not let finished milestones pile up uncommitted.
Write the commit message in the repo's existing style.

## Code style (Java-like)

Follow the Java conventions the language is built around:

- 4-space indentation (no tabs), K&R braces (opening brace on the same line).
- Types spelled out on every declaration: `int n`, `void worker(int k)`,
  `string s`, `*int buf`.
- Control flow in C/Java form: `for (int i = 0; i < n; i = i + 1) { ... }`,
  `while`, `if`/`else`. For-each: `for (T x : c)` (or `var x`) over arrays
  and any class with `size()`/`get(int)` (std `List`, `Queue`, `HashSet`,
  `CopyOnWriteList`; `HashMap` via `.keys()`/`.values()`). `var` infers the
  declared type from the initializer (requires an initializer); in for-each it
  infers the element type from the target.
- Trailing parameters may carry default values
  (`void f(int a, int b = 0)`; called as `f(1)` or `f(1, 2)`). Defaults
  must be constants (literals, enum variants, or `Class.staticField`
  references) — no calls or expressions — and are not allowed on
  constructors or lambda parameters.
- Naming: classes `PascalCase` (`Num`, `Thread`); variables and fields
  `snake_case` (`grand_count`).
- **Function naming: every function and method is `camelCase` — including the
  standard functions (builtins such as `sys.threadCreate`, `str.indexOf`)
  and all `std` library methods (`Thread.nCpu`, `Socket.bindPort`,
  `Mem.retain`). Never use `snake_case` for a function name**
  (`nCpu`, not `n_cpu`; `readAll`, not `read_all`).
- One statement per line; keep bodies short.

## Memory (ownership, like Rust Ch 4.1)

Memory is checked at compile time (`src/middle/ownership.rs`, run after
monomorphization): each value has a single owner; when the owner goes out of
scope the value is dropped.

- **Moves:** `T x = y`, `x = y`, `return y`, and `obj.field = y` *move* a
  class object (`Struct`/`Interface`), a `*T` buffer, a `string`, or an
  array — `y` becomes invalid and using it afterwards is a compile error.
  `*int q = p` moves `p`; `string b = a` moves `a`.
- **Borrows:** calls borrow like `&T` — `x.foo()`, `f(x)`, `str.*`,
  and `sys.*` leave their arguments valid; `&x` borrows;
  `this` (the method receiver) never moves. `int`/`bool`/enum are `Copy`.
- **Owned values free themselves:** dropping means a real `munmap` at scope
  end (or reassignment), verified by the `frees` test — no `free` call
  exists. There is no `alloc` either: buffers come only from typed
  initialization (`int buf[n]`, `byte buf[n]`, `short buf[n]`,
  `string s[n]`), array/string literals, and calls that return fresh
  values. `*int q = p` still moves `p` (raw pointers never own and are
  never freed).
- **Never store a view in an owned slot:** `buf + off`, `&x`, `*p`, and
  `@f` have no header to retain, so passing one where an owned value is
  required (argument, field, return, binding) is a compile error —
  materialize it first (e.g. `str.substring`).
- **Clone explicitly with `x.move()`:** `x.move()` is available on any
  object, `string`, or array value — it borrows the receiver and returns a
  fresh owner of the same type (on objects just `mem.retainVal`, which still
  works as a free function and is what the compiler emits for lambda
  captures; on strings/arrays a plain shared alias). Use it for shared
  ownership: `Pair c = h.move();` keeps `h` valid, as does
  `string b = a.move();`. Use `str.copy(s)` for an independent string.
  Raw `*T` buffers have no `move()` (they follow explicit free discipline).
  A user-defined `move` method wins over this builtin clone
  (e.g. `Point.move(dx, dy)` keeps working); `move()` with args, on a
  class name, or on any other type is a compile error.
- **Collections are the `unsafe` core:** `src/stdlib/coll.flint` stores
  elements as raw `int` slots with manual `Mem.retain`/`Mem.release`, so
  `add`/`put`/`push` borrow (the collection clones internally) while plain
  `T x = y` for objects still moves.

## Writing examples

Examples (`examples/*.flint`) are the language's public face. They must read
like ordinary application code, **not** like a systems-programming demo:

- **Keep `sys.*` built-in calls to an absolute minimum** — that includes
  using pointers and the raw `sys.read`/`write`/`syscall` pass-throughs.
  Byte/short access uses the element-sized buffer types (`byte buf[n]`,
  `short buf[n]`, `string s[n]`, indexing and view casts) instead of raw
  ops. When an example needs a raw `sys.*` call, wrap it in a `std`
  class instead of calling it inline.
- **Do not use `alloc` in an example.** There is no `alloc`: declare typed
  buffers (`int buf[n]` for 8-byte slots, `byte buf[n]`/`string s[n]` for
  bytes, `short buf[n]` for shorts); owned values free themselves
  at scope end, so there is no `free` either.
- **Use the high-level stdlib API** (`src/stdlib/*.flint`). Do **not** call the
  low-level primitives directly in an example:
  - `sys.syscall`, `sys.read`, `sys.write`, `sys.mmap`, ...
- If a capability an example needs is missing from the stdlib, **add a stdlib
  wrapper for it** (in `package std;`, e.g. `class Thread`) and have the example
  call that wrapper. Keep the raw `sys.*` calls *inside* the stdlib
  module, where they belong. Models: `thread.flint` (Thread: nCpu/spawn/join),
  `net.flint` (Socket: stream/bindPort/bindHost/listen/accept/connectHost/
  sendAll/recv/close/nthDot), `sync.flint` (Sync: lock/unlock/cas/nanosleep),
  `mem.flint`
  (Mem: retain/release), `file.flint` (File: readAll/writeAll/
  copy/size/...).
- Show the build line in the header comment, including the stdlib files it
  needs (e.g. `flintc src/stdlib/thread.flint examples/...flint -o ...`).
- **No example uses raw `alloc` / `free` / `memcpy`** (they no longer
  exist): declare typed buffers (`int buf[n]`, `byte buf[n]`,
  `short buf[n]`, `string s[n]`) and use the stdlib (`File`, `Socket`,
  `Thread`, `Sync`) for any other capability. `strings2.flint` also
  *documents* the string indexing API. `byte_access.flint` (a test, not
  an example) is the low-level memory reference (typed buffers, element
  indexing, view casts, `.move()`, syscalls).
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
