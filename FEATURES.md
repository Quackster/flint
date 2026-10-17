# Flint — Language Features

A complete inventory of the Flint language and its toolchain, gathered from the
compiler source (`src/`), the prelude (`src/prelude/`), the standard library
(`src/stdlib/`), the freestanding runtime (`src/intrinsics.s`), the examples
(`examples/`), and the test suite (`tests/`). This is the authoritative
feature list — where the HTML docs (`docs/`) and this file disagree, the
compiler source is the ground truth.

**What Flint is.** A small, statically-typed, systems-level language that
compiles to freestanding x86_64 Linux assembly (no libc). The `flintc`
compiler is written in Rust with zero external dependencies.

- **Source:** `*.flint`
- **Pipeline:** `lexer → parser → desugar (lambdas) → monomorphize → ownership
  check → codegen → .s → as → ld -e _start`
- **Runtime:** a hand-written freestanding `intrinsics.s` (raw `syscall`s,
  heap, refcounting, threads, sockets, print helpers) linked into every
  binary. The collections are ordinary stdlib classes (`src/stdlib/coll.flint`),
  not part of the runtime.

---

## 1. Types

| Type | Notes |
| --- | --- |
| `int` | A single 64-bit integer. The keywords `byte`, `short`, `int`, `long`, and `char` all **collapse to `int`** as scalar types. |
| `boolean` | 0 / 1. |
| `string` | A NUL-terminated byte buffer (a pointer). Indexable by byte. |
| `*T` | Raw pointer. `*int`, `*string`, or untyped `*` (null, `@fn`, conforms to any `*T`). |
| `T[]` / `T a[]` | Untyped array of 8-byte slots (pointer to slots; slot 0 is a length header). Rank is untracked in v1. |
| `byte b[]` / `char c[]` | `ByteArray` — `n` contiguous 1-byte elements. |
| `short s[]` | `ShortArray` — `n` contiguous 2-byte little-endian elements. |
| `void` | Absence of a return value. |
| `ClassName` | A class (struct) type. |
| `InterfaceName` | An interface type — any implementing object satisfies it (used in declarations, casts, `instanceof`). |
| `EnumName` | An enum type — a 64-bit integer value. |
| `(T1, T2, …)` | A **tuple type** — a multi-value return type (≥ 2 members). |
| `T` (type param) | A generic type parameter (resolved to a concrete type by monomorphization). |
| `type NAME = …` | A **type alias** (top-level, resolved by the parser/mono pass). |

**`var`** — type inference from the initializer: `var x = 5;` (requires an
initializer). In a for-each, `var x` infers the element type from the target.
`var (a, b) = …` infers tuple element types.

**Array declaration syntax** — both styles are accepted:

- C#-style prefix: `int[] a`, `int[][] g`, `byte[] b`, `short[] s`
- C-style suffix: `int a[]`, `int g[][]`, `byte b[]`, `short s[]`

Sized buffers cannot be class fields (layouts are fixed); assign `int[N]` in a
constructor instead.

---

## 2. Literals

- **Integer literals:** decimal (`42`, `18446744073709551615` wraps to
  two's-complement `i64`) and **hex** (`0xFF`, `0x8000000000000000`,
  `0xFFFFFFFFFFFFFFFF` — parsed as `u64` then wrapped to `i64`, so the full
  64-bit range is representable).
- **Boolean literals:** `true`, `false`.
- **String literals:** `"text"`, with escapes `\\n \\t \\r \\" \\\\ \\0` and
  **escaped braces** `\\{ \\}` (literal text, no interpolation).
- **String interpolation:** `"a={x}b"` — literal text fragments alternate with
  interpolated expressions (the lexer splits at each `{`, the parser reassembles
  into an `Interp` node).
- **Char literals:** `'a'`, `'\n'`, `'\t'`, `'\r'`, `'\0'`, `'\''`, `'\"'`,
  `'\\'`. A char is a **code point** — the literal is emitted as an `int`.
- **`null`** literal.
- **Enum variant reference:** `EnumName.Variant` — a compile-time integer
  constant.

---

## 3. Variables & scoping

- **Declaration:** `Type name = value;` (an initializer is required).
- **Flat scope — no block shadowing:** a new `Type name` declaration inside an
  inner block **re-declares the same variable**, overwriting it; the new value
  persists after the block ends (see `examples/scope.flint`).
- **Sized buffers:** `int x[n];`, `byte x[n];`, `short x[n];`, `string s[n];`
  — a fresh, zero-filled heap buffer of `n` elements/bytes. No initializer
  (buffers start zero-filled). One-dimensional only. The variable's type is the
  buffer's element type. Also usable as an expression: `int[n]`, `byte[n]`,
  `short[n]`, `char[n]`, `string[n]`.
- **`len(b)`** — the element count of any buffer.
- **String indexing:** `s[i]` reads the byte at offset `i`.
- **`++` / `--`** on an lvalue. **Deliberate departure from C/Java:** both
  `x++` and `++x` evaluate to the value **after** the increment (there is no
  "old value" form).

---

## 4. Operators

- **Arithmetic:** `+  -  *  /  %` — integer division and modulo truncate toward
  zero (C semantics); the result of `%` carries the sign of the dividend.
- **Bitwise:** `&  |  ^  ~  <<  >>`.
- **Comparison:** `==  !=  <  >  <=  >=`.
- **Logical:** `&&` and `||` **short-circuit** and yield 0 / 1.
- **Ternary:** `c ? a : b` — right-associative.
- **Compound assignment** (target must be an lvalue): `+=  -=  *=  /=  %=  &=
  |=  ^=  <<=  >>=`.
- **String operations:** `string + string` (concat), `string + int`
  (concat + itoa) — via `str.concat` / `str.concati`.

Standard precedence.

---

## 5. Control flow

- `if` / `else`.
- `while`.
- `for (init; cond; update)` — all three parts optional.
- **For-each:** `for (T x : c)` (or `var x`) over arrays and any class with
  `size()`/`get(int)` (std `List`, `Queue`, `HashSet`, `CopyOnWriteList`;
  `HashMap` via `.keys()`/`.values()`). The monomorphizer desugars it into an
  index loop.
- **Switch:** `switch (t) { case v: … default: … }` — int-constant cases,
  fall-through allowed.
- **Labeled loops:** `label: for/while …`; `break label` exits that loop from
  anywhere inside it; `continue label` jumps to the next iteration of that
  (outer) loop (see `examples/labels.flint`).
- `break` / `continue` (plain or labeled).
- **`defer expr;`** — `expr` is evaluated when the enclosing function exits
  (Go-style: last deferred runs first, runs on every return path).

---

## 6. Functions

- **Free functions:** `int name(int a, string s) { … }` (an optional return
  type; `void` or omitted).
- **Generic free functions:** `T identity<T>(T x) { … }`.
- **Default parameter values:** trailing parameters may carry defaults
  (`void f(int a, int b = 0)`; called as `f(1)` or `f(1, 2)`). Defaults must
  be **constants** (literals, enum variants, or `Class.staticField` references —
  no calls or expressions) and are **not allowed on constructors or lambda
  parameters**. Once a parameter carries a default, all following ones must too.
- **Static methods** (`static`), **instance methods**, **constructors**
  (`Name(…)` or `void Name(…)`).
- **`async` functions:** a call runs the body on a worker thread and evaluates
  to a `std.Task` (see Concurrency).
- **Multi-value returns:** a function may return a tuple type
  `(T1, T2, …)`; the caller destructures it (see Tuples).
- **Qualified generic free-function calls** with explicit type args:
  `std.map<T, U>(xs, f)`.

---

## 7. Classes & OOP

- **Fields:** `int n;`, `string s;`, with optional `public` / `private`
  visibility, optional `static`, and optional **accessor** `get` / `set` /
  `getset` (properties).
- **Constructors:** `public Name(int x) { … }` (or `void Name(…)`).
- **Methods:** instance and `static`; optional `public` / `private`; optional
  `async`; optional `abstract`.
- **Inheritance:** `class Child extends Parent { … }` — `super` as a receiver
  (`super.field`, `super.method(…)`) and `super(args)` to invoke the parent
  constructor.
- **Interfaces:** `interface Name { void m(…); }` — a set of abstract method
  signatures (v1: no fields, no generic interfaces, no interface inheritance).
  A class adopts them with `implements A, B, …`.
- **Abstract classes:** `abstract class Name { … }` — cannot be instantiated;
  may declare abstract methods (a concrete class must override them).
  `abstract` methods are declared with `;` and no body.
- **`instanceof`:** `e instanceof Type` — true when `e`'s dynamic type is
  `Type` or a subtype.
- **Casts:** `(Type) e` — up/downcast a class value (upcast is a no-op;
  downcast is unchecked in v1). `(byte)` / `(short)` / `(char)` reinterpret a
  buffer in place (a byte/short view of the same memory); `(int)` is a slot-view
  reinterpret when the source is a buffer.
- **`new`:** `new ClassName(args)` and **struct literals**
  `new ClassName { field: value, … }` (with optional explicit type args,
  e.g. `new Vessel<int>(5)`).
- **`this`** — the method receiver (never moves).
- **Class constants:** `const NAME = <value>;` as a class member, referenced as
  `Class.NAME`.
- **Enums:** `enum Name { A, B = 5, C }` — named integer constants; values are
  sequential when not explicitly set.

---

## 8. Generics

- **Generic classes:** `class Vessel<T> { … }` — instantiated as
  `Vessel<int>`, `Vessel<Pair<int, int>>`, …
- **Generic functions:** `T identity<T>(T x) { … }`.
- **Type inference:** type arguments are inferred from argument types when
  omitted (a type parameter no parameter uses falls back to `int`).
- **Explicit type arguments:** `identity<int>(5)`, `new Vessel<int>(5)`,
  `std.map<T, U>(xs, f)`.
- **Monomorphization:** the middle pass expands every generic type/function that
  is actually used (via a worklist, so nested instantiations like
  `Vessel<Pair<int, string>>` are handled) and rewrites the AST so the backend
  only ever sees concrete types and mangled names.

---

## 9. Lambdas & closures

- **Java-style lambda syntax:**
  - `int x -> boolean { … }` (single param, parenless)
  - `(int a, int b) -> int { … }` (param list)
  - `() -> void { … }` (no params)
- **Lifting:** the middle pass lifts a lambda into a generated top-level
  function (whose first parameter is the `*int` capture block) and replaces it
  with a **Closure** — a pointer to a fresh heap block
  `[fn_ptr, cap0, cap1, …]`. Captures are evaluated (by value) when the block
  is created. Calling convention: `fn(ctx, arg1, arg2)`.
- **Calling a closure:** `sys.fnCall2(block, a)` (one arg) and
  `sys.fnCall3(block, a, b)` (two args) — `block` is `[fn_ptr][cap0][cap1]…`.
- **Higher-order stdlib combinators** (pass a lambda value as a `*int`):
  - `std.map<T, U>(xs, f)` — fresh `List<U>` with `f` applied to each element.
  - `std.reduce<T, U>(xs, f, init)` — fold: `f(acc, elem) -> acc`.
  - `std.forEach<T>(xs, f)` — call `f(elem)` for every element (no result).
  - `Arrays.filter(a, pred)` — the elements of a raw `int[]` for which the
    predicate closure is true, as a fresh `int[]`.
  - `List`/`Queue`/`CopyOnWriteList` `.filter(*int pred)` and `.forEach(*int f)`.

---

## 10. Memory model

Ownership is checked at compile time (`src/middle/ownership.rs`, run after
monomorphization) — like Rust Ch 4.1: each value has a single owner; when the
owner goes out of scope the value is dropped.

- **Moves:** `T x = y`, `x = y`, `return y`, and `obj.field = y` *move* a class
  object, a `*T` buffer, a `string`, or an array — `y` becomes invalid and
  using it afterwards is a compile error. `*int q = p` moves `p`;
  `string b = a` moves `a`.
- **Borrows:** calls borrow like `&T` — `x.foo()`, `f(x)`, `str.*`, and `sys.*`
  leave their arguments valid; `&x` borrows; `this` never moves. `int` / `bool`
  / enum are `Copy`.
- **Owned values free themselves:** dropping means a real `munmap` at scope end
  (or reassignment), verified by the `frees` test — no `free` call exists. There
  is no `alloc` either: buffers come only from typed initialization
  (`int buf[n]`, `byte buf[n]`, `short buf[n]`, `string s[n]`), array/string
  literals, and calls that return fresh values. Raw `*T` buffers have no
  `move()` (they follow explicit free discipline).
- **Never store a view in an owned slot:** `buf + off`, `&x`, `*p`, and `@f`
  have no header to retain, so passing one where an owned value is required
  (argument, field, return, binding) is a compile error — materialize it first
  (e.g. `str.substring`).
- **Clone explicitly with `x.move()`:** available on any object, `string`, or
  array value — it borrows the receiver and returns a fresh owner of the same
  type (on objects `mem.retainVal`; on strings/arrays a plain shared alias).
  Use it for shared ownership: `Pair c = h.move();` keeps `h` valid, as does
  `string b = a.move();`. Use `str.copy(s)` for an independent string. A
  user-defined `move` method wins over this builtin clone; `move()` with args,
  on a class name, or on any other type is a compile error.
- **`destroy()` hook:** a class may define `destroy()`, called automatically
  when the owner goes out of scope (the collections use it to release element
  references).
- **Escape analysis** (`src/backend/escape.rs`): decides per class-typed local
  whether its object is stack-allocated (non-escaping) or heap-allocated with a
  refcount header `[refcount][field0][field1]…`. An object escapes (forces the
  heap) when it is passed as an argument, is the base of a method call, is
  returned, is stored in a field, is address-taken (`&x`), or is the target of
  an assignment. Copies retain (`flint_retain`); drops release (`flint_release`).
  Cycles leak in v1 (no weak references).
- **Refcounting primitives:** `mem.retain(o)`, `mem.release(o)`,
  `mem.retainVal(o)` (null-guarded, returns `o`), `mem.retainStr(v)` /
  `mem.releaseStr(v)` (strings held as raw int slots — the collections' unsafe
  core).
- **Raw pointers:** `*T` — `null`, `&x` (address-of), `*p` (deref), `@f`
  (function address, via `sys.fnAddr` / `flint_fn_addr`), and calling through a
  pointer (`flint_call_ptr`).
- **Sized load/store & direct syscalls** for raw memory access.

---

## 11. Exception handling

- **`throw expr;`** — stores `expr` in the global exception slot and returns.
- **`try { … } catch (T var) { … }`** / **`catch (A | B var) { … }`** —
  multi-type catch; the catch variable is typed by the least upper bound (a
  common parent class or interface).
- **`finally { … }`** — runs on normal completion, on a caught exception, and on
  a `return` from the try/catch blocks.

---

## 12. Pattern matching (`match`)

`match <scrutinee> { <pat> => <expr>, … }` — an expression (desugared by the
monomorphizer into a nested `Cond` chain). Supported scrutinee types: `int`,
`char` (a code point), `enum`, and `string`.

- **Patterns:** an integer (or char) literal, a string literal, an enum variant
  (`Enum.Variant`), and the wildcard `_` (matches anything; the fallback).

---

## 13. Tuples (multi-value returns)

- **Tuple type:** `(T1, T2, …)` — a multi-value return type (≥ 2 members).
- **Tuple literal:** `(e1, e2, …)` — meaningful as a `return` value or the RHS
  of a tuple destructure.
- **Destructure into fresh locals:** `(T1 a, T2 b) = value;`
- **Destructure with inferred types:** `var (a, b) = value;`
- **Destructure into existing locals:** `(a, b) = value;`

---

## 14. Concurrency

- **`async` functions/methods:** a call runs the body on its own worker thread
  and evaluates to a `std.Task`.
- **`Task`** (`import std.Task`): `join()` blocks until the worker finishes and
  returns its value (0 for a void function).
- **`await e`:** blocks until the task finishes and evaluates to the value it
  produced. `e` is a call to an `async` function/method (free, instance, or
  static) or an expression of type `std.Task`. A void function yields 0;
  awaiting a bare `Task` yields an int. An async function may await another.
  **At most 32 tasks** may be in flight at once; spawning a 33rd waits for a
  slot to free.
- **`Thread`** (`import std.Thread`): `nCpu()`, `spawn(fn, arg)`, `join(id)`,
  `sleep(millis)` — over `sys.threadCreate` / `sys.threadJoin`.
- **`Sync`** (`import std.Sync`): `lock(*int m)`, `unlock(*int m)`,
  `cas(*int x, expect, value)`, `nanosleep(sec, nsec)` — over
  `sys.mutexLock` / `sys.mutexUnlock` / `sys.atomicCas` / `sys.nanosleep`.
- **Raw primitives:** `sys.clone`, `sys.futex`, `sys.nanosleep`.

---

## 15. Fixed-point math (Q48.16)

No floating point — a **Q48.16 fixed-point** numeric model: a 64-bit int whose
lower 16 bits are the fraction (0.5 == 32768; 1.0 == 65536). `math.*` builtins
operate on these values:

- `math.fadd(a, b)`, `math.fsub(a, b)`, `math.fmul(a, b)`, `math.fdiv(a, b)`
- `math.itof(n)` — int → fixed-point; `math.ftoi(f)` — fixed-point → int
  (rounds half-up).
- `math.fcmp(a, b)` — returns `< 0`, `0`, `> 0` (like `str.cmp`).

(See `examples/fixedpoint.flint`.)

---

## 16. Builtin namespaces

A set of dotted builtin namespaces provides I/O, memory, strings, math, and
utilities. They are compiled directly to the runtime — **no import needed**.
They are separate from the standard-library classes (ordinary classes in
`package std` that you import).

| Namespace | Functions |
| --- | --- |
| `sys` | `open read write close brk exit syscall` · `socket bind listen accept connect sockaddr` · `select poll epollCreate1 epollCtl epollWait` · `clone futex nanosleep` · `threadCreate threadJoin` · `mutexLock mutexUnlock atomicCas` · `mmap munmap` · `fnCall2 fnCall3` |
| `str` | `cmp concat concati copy itoa len replace substring format` |
| `math` | `itof ftoi fadd fsub fmul fdiv fcmp` (fixed-point) |
| `mem` | `retain release retainVal retainStr releaseStr` |
| `io` | `print println` |
| `conv` | `atoi` |
| `time` | `millis now` |
| `rand` | `next range` |
| `env` | `get` |
| `log` | `info warn error debug` |
| `b64` | `encode decode` |
| `json` | `get geti` |

Notes:

- `print` / `println` each take a string or an int (an int argument is formatted
  in decimal) and write to stdout; `println` adds a trailing newline, `print`
  does not.
- `log.*` writes tagged lines — `log.info` to stdout, `log.warn` to stderr.
- `str.format(fmt, a, b, c, d)` is a varargs-style formatter supporting `%d`
  (int), `%s` (*byte), `%b` (int as bool) placeholders.
- `sys.syscall(num, a1, a2, a3, a4, a5)` is the generic syscall escape hatch
  (the 4th syscall argument arrives in `%rcx` and is moved to `%r10`).
- `sys.mmap(addr, len, prot, flags, fd, offset)` — `PROT_READ|PROT_WRITE = 3`;
  `MAP_SHARED = 1`, `MAP_PRIVATE|MAP_ANON = 0x22`.

---

## 17. Standard library (`package std`)

Ordinary classes in `package std` that you `import`. Each file declares
`package std;`.

| Class | File | Highlights |
| --- | --- | --- |
| `Collection` | `coll.flint` | Abstract base for the collections; raw 8-byte slot storage, element `kind` (0 int/bool/enum/ptr/array, 1 string, 2 object), `size()`/`isEmpty()`/`clear()`/`destroy()`. |
| `List<T>` | `coll.flint` | `add` `insert` `get` `set` `remove` `contains` `clear` `sort` `reverse` `join` `forEach` `filter`. Grows by doubling and copying. |
| `Queue<T>` | `coll.flint` | FIFO ring: `push` `pop` `peek` `clear` `forEach`. |
| `HashSet<T>` | `coll.flint` | Open addressing (linear probing, tombstones): `add` `get` `contains` `remove` `clear` `rehash`. |
| `HashMap<K, V>` | `coll.flint` | `put` `get` `contains` `remove` `clear` `rehash`; `kkind`/`vkind`. |
| `CopyOnWriteList<T>` | `coll.flint` | Concurrent: every mutator builds a fresh slot array and swaps it in under a lock; readers and `snapshot()` (a plain `List<T>`) never lock. `add` `insert` `set` `remove` `removeObject` `clear` `get` `contains` `indexOf` `lastIndexOf` `forEach` `join` `filter` `snapshot`. |
| `map` / `reduce` / `forEach` | `collect.flint` | Higher-order combinators over `List<T>` (see Lambdas). |
| `ReduceBox<V>` | `collect.flint` | One-field box for the reduce accumulator. |
| `Arrays` | `array.flint` | `filter(a, pred)` over raw `int[]`. |
| `Bit` | `bit.flint` | `popcount nlz ntz setBit clrBit testBit toggleBit bitRev isPowerOfTwo nextPowerOfTwo byteSwap parity getByte setByte`. |
| `Checksum` | `checksum.flint` | `crc32` (zlib/IEEE, table computed once), `sum32`, `djb2`. |
| `File` | `file.flint` | `open openForWrite close size readAll writeAll exists delete append copy rename readLine`. |
| `Path` | `path.flint` | `lastSlash lastDot base dir ext join parent isAbsolute normalize`. |
| `Rand` | `rand.flint` | `init next range coin shuffle pick bytes hexId randString`. |
| `Sort` | `sort.flint` | `sort sortDesc reverse min max sum contains indexOf count fill binarySearch argmin argmax swap avg rotate`. |
| `Str` | `str.flint` | `upper lower reverse trim ltrim rtrim count contains lastIndexOf startsWith endsWith replaceAll repeat join ljust rjust`. |
| `Num` | `num.flint` | `toString hexDigit hexVal toHex fromHex toBin fromBin toOct fromOct sumDigits digitCount reverseDigits trimLeadingZeros isPalindrome digitAt toUpperHex`. |
| `Math` | `math.flint` | `abs min max clamp gcd lcm factorial isPrime sqrt pow fround fceil ffloor fabs fsqrt fib cbrt isEven isOdd mod sumRange`. |
| `Mem` | `mem.flint` | `retain release` (over `mem.retain` / `mem.release`). |
| `Sync` | `sync.flint` | `lock unlock cas nanosleep`. |
| `Thread` | `thread.flint` | `nCpu spawn join sleep`. |
| `Task` | `task.flint` | `join()` — the handle for `async` calls. |
| `Time` | `time.flint` | `millis seconds nanos date`. |
| `Socket` | `net.flint` | `stream bindPort bindHost nthDot listen accept connectHost sendAll recv close` (TCP over `sys.*`). |
| `Image` | `image.flint` | RGBA pixel buffer: `rgba rgb cr cg cb ca get set setAlpha r g b a fill clear invert grayscale flipH flipV blur hline vline fillRect rect line circle fillCircle text textWidth toPpm toPgm toBmp save readFile toPng` (built-in PNG encoder + zlib inflate). |
| `Window` | `gui.flint` | Wayland window: `ok running width height close minimize nextEvent peekEvent pump evType evX evY evButton evKey evState evMods nowMs present`; event kinds `FRAME KEY BUTTON MOTION CLOSE CONFIGURE`. |
| `App` | `gui.flint` | `run stop running noEscapeQuit` — the event-loop host for GUI apps. |
| `OnSetup` `OnDraw` `OnKey` `OnButton` `OnMotion` `OnResize` `OnClose` | `gui.flint` | Event-handler interfaces a GUI class `implements`. |
| `Widget` + subclasses | `widget.flint` | `Label TextBox PasswordBox NumberBox TextArea Button Checkbox RadioButton Slider Dropdown` — a small widget toolkit over `Image`. |

---

## 18. Modules & multi-file

- **Packages:** `package a.b.c;` at the top of a file (dotted path, no leading
  dot; empty = default package).
- **Imports:** `import a.b.Foo;` (a fully qualified name) or
  `import a.b.*;` (wildcard). Collected across all input files.
- **Multi-file:** `flintc a.flint b.flint -o out` — files are concatenated
  (each keeps its own `package` header).
- **Manifest:** `flintc --manifest box.toml -o out` — read source files from a
  TOML manifest (`sources = ["util.flint", "main.flint"]`).
- **Directory:** `flintc --dir src -o out` — compile every `.flint` in a
  directory (sorted).
- **Top-level constants:** `const NAME = <value>;` (int, bool, or string),
  scoped by `package`; inlined at use sites by the monomorphizer.
- **Type aliases:** `type NAME = <type>;` (top-level), resolved by the parser.

---

## 19. The compiler & CLI

**Pipeline:** `lexer → parser → desugar (lambdas) → monomorphize → ownership
check → codegen → .s → as → ld -e _start`.

- **middle/** — lambda desugaring (lift a lambda to a function taking its
  captures; replace the node with a closure); monomorphization (`ensure`,
  `expand`, `infer`, `resolve`, `resolve_expr`); ownership checking
  (`ownership.rs`).
- **backend/** — code generation (`codegen/`: `accessor`, `binop`, `builtin`,
  `call`, `ctx`, `expr`, `func`, `lvalue`, `new`, `stmt`) plus `escape.rs`
  (stack-vs-heap escape analysis), `layout.rs`, and `release.rs`.
- **intrinsics.s** — the freestanding runtime, embedded at compile time and
  linked into every binary (raw `syscall`s, heap, refcount, threads, sockets,
  print helpers, the `destroy()` hook).

**`flintc` CLI flags:**

- `-o <FILE>` — write output to `FILE` (default: first input without `.flint`).
- `-S` — stop after generating assembly (print to stdout, or to a file with
  `-o out.s`).
- `--manifest <FILE>` — read source files from a TOML manifest.
- `--dir <DIR>` — compile every `.flint` in `DIR` (sorted).
- `-h` / `--help` — print usage.

**Build & test:**

```sh
cargo build                 # -> target/debug/flintc (release: --release)
bash tests/run_tests.sh     # full suite (expect: PASS=126  FAIL=0)
```

Requires `as` and `ld` (binutils) on the PATH.

---

## 20. Deliberate departures & v1 limitations

- **`x++` / `++x`** both evaluate to the value **after** the increment (no
  "old value" form) — a departure from C/Java.
- **Flat scope** — no block-level shadowing (re-declaration overwrites).
- **Downcasts are unchecked** (trust the programmer).
- **Cycles leak** — no weak references.
- **No `alloc` / `free` / `memcpy`** — typed buffers only; owned values free
  themselves at scope end.
- **No method-level generics** in v1 (a method's `type_params` is always empty);
  generic free functions are the mechanism for polymorphic calls
  (`std.map<T, U>`, `identity<T>`).
- **No generic interfaces, no interface inheritance, no enum associated data** in
  v1.
- **Array rank is untracked** in v1.
- **At most 32 tasks** in flight at once.
- **No floating point** — Q48.16 fixed-point only.
