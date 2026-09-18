# flint / flintc

A small, statically-typed, systems-level language that compiles to
freestanding x86_64 Linux assembly (no libc). The `flintc` compiler is written
in Rust with zero external dependencies.

- Syntax: Java style conventions (classes, `int`/`void`, braces, 4-space
  indent); all code samples below are Java and highlighted as such.
- Source: `*.flint`
- Pipeline: `lexer -> parser -> monomorphize -> codegen -> .s -> as -> ld -e _start`
- Runtime: a hand-written freestanding `intrinsics.s` + `collections.s`
  (raw `syscall`s, heap, refcount, threads, sockets, print helpers) linked in
  by the driver.

## Quick start

```sh
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

## Style guide (Java)

Flint follows Java's formatting conventions; every code sample in this README is
written to match. Follow the same conventions so your sources read like the docs.

- **Indent:** 4 spaces per level (no tabs).
- **Braces:** K&R — the opening brace sits on the same line for functions,
  classes, and control flow; `} else {` stays on one line. Every `if` / `while`
  / `for` / `try` / `catch` takes braces, even for a single statement.
- **Statements:** one per line.
- **Spacing:** a space between a type and its name (`int x`), spaces around
  binary operators (`a + b`, `x == y`), a space after commas, no space before a
  `(` or after a `{`.
- **Line length:** keep lines under 80 columns where practical; wrap long
  expressions at a logical boundary and indent the continuation.
- **Blank lines:** one between the field block and the methods, and one between
  methods.
- **Naming:** PascalCase for classes and enums (`Point`, `Color`); camelCase for
  methods, fields, and locals (`doubleIt`, `getAge`); `UPPER_SNAKE_CASE` for
  `static` constants.
- **Visibility:** `public` / `private` prefix fields and methods; the default is
  `public`.
- **Class layout:** fields first, then the constructor, then methods.
- **Types first:** declarations put the type before the name
  (`int sum(int a[], int n)`, `string s = "hi"`); arrays are written `T a[]`.
- **Entry point:** a top-level `int main() { ... }` returning the exit code
  (`return 0;`); `void` for no return value.
- **Constants:** prefer a `static` class field (e.g. `static int LIMIT;`) over a
  magic number.
- **Comments:** `//` for line comments and `/* ... */` for blocks.

```java
class Point {
    private int x;
    private int y;

    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }

    public int sum() {
        return x + y;
    }
}

int main() {
    Point p = new Point(3, 4);
    if (p.sum() > 5) {
        printi(p.sum());
    }
    return 0;
}
```

## Features

A compact overview; each category is expandable.

<details>
<summary>Core language & types</summary>

- Statically typed, Java-flavored syntax, systems-level semantics.
- `byte short int long char` are all 64-bit in v1; `boolean` is `i8` (0/1).
- Integer literals are decimal or `0x` hex; hex values up to
  `0xFFFFFFFFFFFFFFFF` are parsed as unsigned and wrap to two's complement
  (e.g. `0xFFFFFFFF` is `-1`, `0x8000000000000000` is the most negative int).
- `T a[]` arrays, `string` (NUL-terminated byte sequence), `void`, and a
  `null` value.
- Raw pointers (`*T`, `&x`, `*p`) are a low-level escape hatch (see
  Pointers & raw memory); standard code uses `string` and builtins.
- Class names are reference types (heap objects); collections are reference types.
- Enums: `enum Color { Red, Green, Blue }`; values default to 0, 1, 2 (or
  set them explicitly).
- Comments: `// line` and `/* block */`.

```java
enum Color { Red, Green, Blue }
int main() {
    int n = 42;
    string s = "hello";        // NUL-terminated
    int a[] = [1, 2, 3];       // array
    Color c = Color.Green;     // c == 1
    printi(n);                 // 42
    printi(strlen(s));         // 5
    printi(len(a));            // 3
    printi(c);                 // 1
    return 0;
}
```
</details>

<details>
<summary>Functions & methods</summary>

- Free functions: `int fib(int n) { ... }` (return type first, Java-style).
- Methods take an implicit `this`; bare `x` inside a method desugars to `this.x`.
- Static methods: `static int name(...)`; call them by class name,
  `Math.abs(7)`: no instance needed (calling a non-static method that way
  is a compile error).
- Calls chain: `p.move(1, 1).getX()`.
- Properties generate accessors from a field name: `get` / `set` / `getset`.
- Constructors: a method named after the class; `new Name(args)` routes by arity.
- Varargs-style formatting: `str.format("%d + %d = %d", a, b, c)` (`%d`, `%s`, `%b`).

```java
int fib(int n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

class Point {
    int x;
    int y;

    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }

    public int sum() {
        return x + y;
    }
}

int main() {
    printi(fib(10));          // 55
    Point p = new Point(3, 4);
    printi(p.sum());         // 7
    return 0;
}
```
</details>

<details>
<summary>OOP & polymorphism</summary>

- Inheritance: `class Dog extends Animal`, `super(...)`, `super.method()`, overriding.
- Abstract classes: `abstract class Shape`, `abstract int area();`.
- Interfaces: `interface Speaker`, `class Robot implements Speaker`.
- `instanceof` test and casts: `Dog d = (Dog) a;`, `a instanceof Dog`.
- Static fields: `static int n;` accessed as `Counter.n`; static methods
  called as `Math.abs(7)`.
- Generics: `class Vessel<T>`, `T identity<T>(T x)`: monomorphized at compile time.

```java
class Animal {
    int id;

    public Animal(int a) {
        this.id = a;
    }

    public int speak() {
        return 0;
    }
}

class Dog extends Animal {
    int barks;

    public Dog(int a, int b) {
        super(a);
        this.barks = b;
    }

    public int speak() {            // override
        return 1;
    }
}

interface Speaker {
    int speak();
}

class Robot implements Speaker {
    public int speak() {
        return 10;
    }
}

class Vessel<T> {
    T v;
}

int main() {
    Animal a = new Dog(5, 2);
    printi(a.speak());                 // 1  (virtual dispatch)
    printi(a instanceof Dog);         // 1
    Speaker s = new Robot();
    printi(s.speak());                 // 10
    Vessel<int> b = new Vessel<int>(41);
    printi(b.v);                      // 41
    return 0;
}
```
</details>

<details>
<summary>Data structures & collections</summary>

- Arrays: `int a[] = [1, 2, 3]`; multi-dimensional via repeated `[]`.
- Built-in collections (typed or untyped): `list`, `queue`, `hashmap`, `hashset`,
  `dictionary` (alias of `hashmap`); keys/values may be ints **or** strings.
- Structs: classes with plain fields, including nested object fields.
- `len(a)` / `len(c)` for array and collection size.

```java
int main() {
    list l = [10, 20, 30];
    l.add(40);
    l.sort();
    l.reverse();
    print(l.join("-"));
    print("\n");                   // 40-30-20-10
    hashmap m = new hashmap();
    m.put("apple", 1);
    m.put("banana", 2);
    printi(m.get("banana"));           // 2
    printi(len(m));                   // 2
    hashset s = new hashset();
    s.add(5);
    s.add(5);
    printi(len(s));                   // 1  (unique)
    queue q = new queue();
    q.push(1);
    q.push(2);
    printi(q.pop());                 // 1  (FIFO)
    return 0;
}
```
</details>

<details>
<summary>Control flow & operators</summary>

- `if`/`else`, `while`, `for` (all parts optional), range-for `for (T x : a)`,
  `switch`/`case`/`default` (fall-through allowed), `break`, `continue`, `return`.
- Arithmetic `+ - * / %`, bitwise `& | ^ << >>`, compare `== != < > <= >=`.
- Unary `- ! * &`, prefix/postfix `++ --`, postfix `.field [idx]`.
- Compound assign `+= -= *= /=`, ternary `c ? a : b`.
- `&&`/`||` are logical with short-circuit (return `0`/`1`).

```java
int main() {
    int x = 0;
    for (int i = 0; i < 5; i = i + 1) {
        if (i == 2) {
            continue;
        }
        if (i == 4) {
            break;
        }
        x += i;
    }
    printi(x);                    // 4  (0 + 1 + 3)
    int a[] = [1, 2, 3];
    int s = 0;
    for (int v : a) {
        s += v;
    }
    printi(s);                   // 6
    switch (s) {
        case 6:
            printi(100);
            break;
        default:
            printi(0);
    }
    printi(2 + 3 * 4);          // 14
    printi(1 || 0);             // 1  (short-circuit)
    printi(0 && 7);             // 0  (short-circuit)
    return 0;
}
```
</details>

<details>
<summary>Memory management</summary>

- Class objects live on the heap with a refcount header
  (`[refcount][field0][field1]...`); copy retains, drop releases.
- Escape analysis: class locals that do not escape are stack-allocated.
- `alloc(n)` / `free(p)` / `memcpy(d, s, n)`: raw heap, used through
  `string` variables.
- Sized loads/stores: `sys.byte_load/store`, `sys.short_load/store`,
  `sys.int_load/store`.
- Raw pointers (`&x`, `*T`, `*p`) are a low-level escape hatch; standard
  code addresses memory through `string` (`buf[i]` = 8-byte slot,
  `sys.byte_*` = sub-word cells).

```java
class Pair {
    int a;
    int b;

    public Pair(int x, int y) {
        this.a = x;
        this.b = y;
    }
}

int use(Pair p) {
    return p.a + p.b;
}

int main() {
    Pair p = new Pair(3, 4);   // escapes into use() -> heap + refcount
    printi(use(p));            // 7
    int x = 10;
    string q = &x;
    q[0] = 99;
    printi(x);                // 99  (write through the address)
    string buf = alloc(32);
    buf[0] = 7;
    printi(buf[0]);          // 7
    free(buf);
    return 0;
}
```
</details>

<details>
<summary>Exceptions</summary>

- `try { ... } catch (T e) { ... }`, `throw <value>`.
- Catch a primitive (e.g. `int`) or a class error type.
- A `try` that does not throw returns normally.

```java
int main() {
    try {
        throw 42;
    } catch (int e) {
        printi(e);            // 42
    } finally {
        printi(1);           // always runs, even on throw
    }
    try {
        printi(7);           // no throw: runs normally
    } catch (int e) {
        printi(e);
    }
    return 0;
}
```
</details>

<details>
<summary>Concurrency</summary>

- `sys.thread_create(@fn, arg)`, `sys.thread_join(tid)`.
- The `@` operator yields a function's address.
- Synchronization: `sys.mutex_lock/unlock(&m)`, `sys.atomic_cas(&v, old, new)`.
- Lower-level: `sys.clone`, `sys.futex`, `sys.nanosleep(sec, nsec)`.

```java
void thread_fn(int arg) {
    printi(arg);             // 7
}
int main() {
    int val = 5;
    sys.atomic_cas(&val, 5, 10);
    printi(val);             // 10  (CAS succeeded)
    int tid = sys.thread_create(@thread_fn, 7);   // @ = fn address
    sys.thread_join(tid);
    return 0;
}
```
</details>

<details>
<summary>Networking (TCP)</summary>

- `sys.socket(fam, type, proto)`, `sys.sockaddr(port, o1, o2, o3, o4)`,
  `sys.bind`, `sys.listen`, `sys.accept`, `sys.connect`.
- Read/write over a socket fd; `sys.close` to release.
- I/O multiplexing: `sys.select`, `sys.poll`, `sys.epoll_create1/ctl/wait`.

```java
int main() {
    int s = sys.socket(2, 1, 0);            // AF_INET, SOCK_STREAM
    sys.bind(s, sys.sockaddr(7777, 0, 0, 0, 0), 16);
    sys.listen(s, 5);
    int c = sys.accept(s, 0, 0);
    string buf = alloc(64);
    int n = sys.read(c, buf, 64);
    sys.write(c, buf, n);                 // echo back
    sys.close(c);
    free(buf);
    sys.close(s);
    return 0;
}
```
</details>

<details>
<summary>I/O & system</summary>

- File I/O: `sys.open(path, flags, mode)`, `sys.read/write`, `sys.close`.
- Standard streams: `print(s)`, `printi(n)`, stdin via `sys.read(0, buf, n)`.
- `env.get("NAME")`, `time.millis()`, `rand.next()` / `rand.range(min, max)`.
- `log.info/warn/error/debug(s)`.
- `b64.encode/decode`, `json.get(s, key)` / `json.geti(s, key)`.
- Generic syscall escape hatch: `sys.syscall(num, a1, a2, a3, a4, a5)`.

```java
int main() {
    int f = sys.open("/tmp/flintc_demo.txt", 0x241, 0x1a4);
    sys.write(f, "hi", 2);
    sys.close(f);
    string p = env.get("PATH");
    printi(p != null ? 1 : 0);             // 1
    printi(time.millis() > 1000000000000 ? 1 : 0);  // 1
    int r = rand.range(10, 20);
    printi(r >= 10 && r < 20 ? 1 : 0);    // 1
    string e = b64.encode("hello");
    printi(str.cmp(e, "aGVsbG8=") == 0 ? 1 : 0);   // 1
    log.info("hello");
    return 0;
}
```
</details>

<details>
<summary>Strings</summary>

- NUL-terminated `string`; literals lower to `.asciz`.
- `str.concat`, `str.concati`, `str.itoa`, `conv.atoi`.
- `str.cmp`, `str.copy`, `str.substring`, `str.indexOf`, `str.replace`.
- `str.format(fmt, a, b, c, d)`: printf-style `%d`, `%s`, `%b`.
- `strlen(s)`; passed and stored by pointer.

```java
int main() {
    string s = "hello" + " world";        // "hello world"
    print(s);
    print("\n");
    string a = "hello";
    string b = "hello";
    printi(a == b);                     // 1  (content compare)
    string sub = str.substring("hello world", 6, 11);
    printi(str.cmp(sub, "world") == 0 ? 1 : 0);   // 1
    printi(str.indexOf("hello world", "world") == 6 ? 1 : 0);  // 1
    string r = str.replace("aaa", "a", "b");
    printi(str.cmp(r, "bbb") == 0 ? 1 : 0);       // 1
    string f = str.format("%d + %d = %d", 1, 2, 3, 0);
    printi(str.cmp(f, "1 + 2 = 3") == 0 ? 1 : 0); // 1
    return 0;
}
```
</details>

<details>
<summary>Floats</summary>

- Fixed-point Q48.16 (64-bit int, lower 16 bits fractional) via `math.*`:
  `math.itof`, `math.ftoi`, `math.fadd`, `math.fsub`, `math.fmul`,
  `math.fdiv`, `math.fcmp`.

```java
int main() {
    int a = math.itof(3);
    int b = math.itof(2);
    printi(math.ftoi(math.fadd(a, b)));   // 5  (3.0 + 2.0)
    printi(math.ftoi(math.fmul(a, b)));   // 6  (3.0 * 2.0)
    printi(math.fcmp(a, b) == 1 ? 1 : 0); // 1  (3.0 > 2.0)
    return 0;
}
```
</details>

<details>
<summary>Modules & multi-file</summary>

- `package com.example;` declares a package; `import com.a.A;` pulls in a type.
- Compile multiple files at once, or via a TOML manifest / a directory.
- Each source file keeps its own `package` header.

```java
// util.flint
package com.example;

class Util {
    public Util() {
    }

    public int doubleIt(int x) {
        return x * 2;
    }
}

// main.flint  (compile:  flintc util.flint main.flint -o out)
import com.example.Util;

int main() {
    Util u = new Util();
    printi(u.doubleIt(21));   // 42
    return 0;
}
```
</details>

<details>
<summary>Standard library (std.*)</summary>

A standard library written entirely in Flint (`package std`, in
`src/stdlib/`), on top of the builtins. Compile it alongside your sources:

```sh
flintc src/stdlib/*.flint myapp.flint -o myapp
```

then `import std.X;` and call `X.method(...)` (static methods, no instance):

```java
import std.Math;
import std.Sort;
import std.Num;

int main() {
    printi(Math.gcd(12, 18));          // 6
    int a[] = [5, 2, 9, 1, 5];
    Sort.sort(a);                     // [1, 2, 5, 5, 9]
    printi(Sort.max(a));            // 9
    print(Num.to_hex(255));         // ff
    print("\n");
    return 0;
}
```

| class | methods |
|-------|---------|
| `std.Math`   | `abs`, `min`, `max`, `clamp`, `gcd`, `lcm`, `factorial`, `is_prime`, `sqrt`, `pow`, `pow10`, `fib`, `cbrt`, `is_even`, `is_odd`, `mod`, `sum_range`, plus fixed-point `fround`, `fceil`, `ffloor`, `fabs`, `fsqrt` |
| `std.Sort`   | `sort`, `sort_desc` (stable, in place), `reverse`, `min`, `max`, `sum`, `contains`, `index_of`, `count`, `fill`, `binary_search`, `argmin`, `argmax`, `swap`, `avg`, `rotate` |
| `std.Bit`    | `popcount`, `nlz`, `ntz`, `set_bit`, `clr_bit`, `test_bit`, `toggle_bit`, `bit_rev`, `is_power_of_two`, `next_power_of_two`, `byte_swap`, `parity`, `get_byte`, `set_byte` |
| `std.Num`    | `to_string`, `to_hex`/`from_hex`, `to_bin`/`from_bin`, `to_oct`/`from_oct`, `to_upper_hex`, `sum_digits`, `digit_count`, `digit_at`, `reverse_digits`, `is_palindrome` |
| `std.File`   | `open`, `open_for_write`, `close`, `size`, `read_all`, `write_all`, `exists`, `delete`, `append`, `copy`, `rename`, `read_line` |
| `std.Path`   | `base`, `dir`, `parent`, `ext`, `join`, `last_slash`, `last_dot`, `is_absolute`, `split`, `normalize` |
| `std.Checksum` | `crc32` (zlib/IEEE `0xEDB88320`), `sum32`, `djb2`, `fnv1a` (64-bit), `fnv1a32`, `adler32` |
| `std.Rand`   | `init(seed)` (xorshift64), `next`, `range(lo, hi)`, `coin`, `shuffle`, `pick`, `bytes`, `hex_id`, `rand_string` |
| `std.Str`    | `upper`, `lower`, `reverse`, `trim`, `ltrim`, `rtrim`, `count`, `contains`, `last_index_of`, `starts_with`, `ends_with`, `replace_all`, `repeat`, `split`, `join`, `ljust`, `rjust` |
| `std.Time`   | `millis`, `seconds`, `nanos`, `date` (`YYYY-MM-DD HH:MM:SS`) |

</details>

## Language reference

Detailed per-topic reference; each topic is expandable.

<details>
<summary>Types</summary>

- `byte short int long char` all 64-bit in v1.
- `boolean` is `i8` with values 0 and 1.
- `*T` is a pointer to `T`.
- `T a[]` is an array of `T` (see Arrays).
- `class` names are reference types (heap objects, see Objects & memory).
- `string` is a NUL-terminated byte sequence (a pointer under the hood);
  raw `*T` pointers are the escape hatch to the same memory.
- `list`, `queue`, `hashmap`, `hashset` (and the `dictionary` alias of
  `hashmap`) are built-in collections (see Collections).
- `void` returns nothing.
- `null` is the null value; class, `*T`, and `T a[]` locals and fields start as `null`.

```java
enum Color { Red, Green, Blue }

class Point {
    int x;

    public Point(int a) {
        this.x = a;
    }
}

int main() {
    int n = 42;
    boolean flag = 1;              // i8, 0/1
    string p = alloc(8);          // byte buffer (raw *T works too)
    int a[] = [1, 2, 3];          // array
    string s = "hi";              // NUL-terminated
    Point pt = null;              // null reference
    Color c = Color.Green;        // c == 1
    printi(n + len(a) + flag + (c == 1 ? 1 : 0));  // 47
    free(p);
    return 0;
}
```
</details>

<details>
<summary>Functions</summary>

- `int fib(int n) { ... }` declares a function; return type first, Java-style.
- `void name() { ... }` for no return value.
- Parameters are `Type name` pairs, comma-separated.
- `return e;` exits with a value; `return;` for `void`.
- Top-level functions are free functions; call them by name.

```java
int fib(int n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

void greet() {
    print("hi");
    print("\n");
}

int main() {
    printi(fib(10));   // 55
    greet();
    return 0;
}
```
</details>

<details>
<summary>Classes & fields</summary>

- `class Name { ... }` holds fields and methods inside one brace block.
- Field: `Type name;` (optional `private`/`public` and accessor prefix).
- Fields are per-instance by default; each object stores one slot per field.
- `Point p = new Point(3, 4);` constructs an object.
- `p.x` reads and writes a field on `p`.
- Field order in `new` follows declaration order.

```java
class Point {
    int x;
    int y;
}
int main() {
    Point p = new Point(3, 4);   // no constructor: positional field init
    p.x = 10;
    printi(p.x + p.y);          // 14
    return 0;
}
```
</details>

<details>
<summary>Methods & this</summary>

- `public int sum() { return x + y; }` declares a method inside a class.
- A method takes an implicit `this` as its first parameter.
- `this.x` is the field on the receiver; a bare `x` inside a method desugars
  to `this.x` when no local or param matches.
- `p.sum()` calls the method with `p` as `this`.
- Calls chain: `p.move(1, 1).getX()` is valid.
- Method names are mangled to `Class_method` in the assembly.

```java
class Point {
    int x;
    int y;

    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }

    public int sum() {                 // bare x = this.x
        return x + y;
    }

    public Point move(int dx, int dy) {
        this.x = this.x + dx;
        this.y = this.y + dy;
        return this;                 // enables chaining
    }
}

int main() {
    Point p = new Point(1, 2);
    printi(p.sum());                          // 3
    printi(p.move(1, 1).sum());              // 5  (chained call)
    return 0;
}
```
</details>

<details>
<summary>Visibility</summary>

- `private` and `public` prefix fields and methods.
- Default visibility is `public`.
- A `private` member is reachable only from inside the same class.
- Access from another class is a compile error with a span.

```java
class Safe {
    private int secret;
    public int open;

    public Safe(int s, int o) {
        this.secret = s;
        this.open = o;
    }

    public int peek() {            // ok: inside the class
        return secret;
    }
}

int main() {
    Safe s = new Safe(7, 3);
    printi(s.peek());   // 7
    printi(s.open);     // 3  (public)
    // printi(s.secret);  // compile error: private to other classes
    return 0;
}
```
</details>

<details>
<summary>Properties</summary>

Accessors generate methods from a field name. The name is converted to
PascalCase after stripping leading underscores; the underscore prefix is
conventional, not required.

- `get` generates `Ty getName()`.
- `set` generates `void setName(Ty value)`.
- `getset` generates both.

```java
class User {
    private get int _age;        // _age -> getAge()
    private getset string _name; // _name -> getName(), setName()
    private getset int score;    // score -> getScore(), setScore()
}
int main() {
    User u = new User(30, "alice", 100);
    printi(u.getAge());            // 30
    print(u.getName());            // alice
    print("\n");
    u.setScore(7);
    printi(u.getScore());         // 7
    return 0;
}
```
</details>

<details>
<summary>Constructors</summary>

- A method whose name equals the class name is the constructor.
- `new Name(args)` with a matching constructor arity routes to it.
- `new Point(3, 4)` with no constructor uses positional field init.
- `new Counter()` with a zero-arg constructor calls the constructor.
- A constructor arity mismatch is a compile error.

```java
class Point {
    int x;
    int y;

    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }
}

class Size {
    int w;
    int h;
}

class Counter {
    int n;

    public Counter() {
        this.n = 100;
    }
}

int main() {
    Point p = new Point(3, 4);    // routes to the 2-arg constructor
    Size s = new Size(800, 600);  // no constructor: positional field init
    Counter c = new Counter();    // zero-arg constructor
    printi(p.x + c.n - s.h / 100);  // 97
    return 0;
}
```
</details>

<details>
<summary>Inheritance, abstract & interfaces</summary>

```java
class Animal {
    int id;

    public Animal(int a) {
        this.id = a;
    }

    public int speak() {
        return 0;
    }
}

class Dog extends Animal {
    int barks;

    public Dog(int a, int b) {
        super(a);
        this.barks = b;
    }

    public int speak() {                    // override
        return 1;
    }

    public int bark() {
        return super.speak() + this.barks;
    }
}

abstract class Shape {
    abstract int area();                    // abstract method
}

class Circle extends Shape {
    int r;

    public Circle(int x) {
        this.r = x;
    }

    public int area() {
        return this.r * this.r;
    }
}

interface Speaker {
    int speak();
}

class Robot implements Speaker {
    int level;

    public Robot(int l) {
        this.level = l;
    }

    public int speak() {
        return this.level * 10;
    }
}

int main() {
    Dog d = new Dog(5, 2);
    printi(d.speak());             // 1  (override)
    printi(d.bark());            // 3  (super + field)
    Circle c = new Circle(3);
    printi(c.area());            // 9
    Robot r = new Robot(2);
    printi(r.speak());           // 20  (interface method)
    return 0;
}
```

- `class Dog extends Animal` inherits fields and methods; `super(...)` calls
  the parent constructor; `super.method()` calls the parent's method.
- Overriding: a subclass re-declares a method with the same name.
- `abstract class` cannot be instantiated; `abstract` methods have no body.
- `interface` declares a method set; `class ... implements Speaker` fulfils it.
- `a instanceof Dog` tests the dynamic type (also true for `interface`).
- Casts: `Dog d = (Dog) a;` narrows a reference; the runtime layout matches.
</details>

<details>
<summary>Static fields</summary>

```java
class Counter {
    static int n;
    int id;

    void Counter(int i) {
        this.id = i;
    }
}

int main() {
    Counter.n = 5;                 // accessed via the class name
    Counter c = new Counter(1);
    Counter.n = Counter.n + 3;     // shared across all instances
    printi(c.id);                 // instance fields still work alongside statics
}
```

- `static Type name;` is a single slot shared by all instances of the class.
- Read and write it as `ClassName.name`; it is not part of any instance layout.
- `static` methods (e.g. `static int abs(int x) { ... }`) need no `this`;
  call them by class name: `Math.abs(7)`. Calling a non-static method through
  the class name is a compile error.
</details>

<details>
<summary>Enums</summary>

```java
enum Color {
    Red, Green, Blue,            // sequential from 0
}
enum Status {
    Active = 10,                // explicit value
    Inactive,                   // continues: 11
}
int main() {
    printi(Color.Blue + 1);     // 3
    printi(Status.Inactive);    // 11
    return 0;
}
```

- Values default to sequential, starting at 0 (or after the last explicit value).
- Usable in expressions: `int x = Color.Blue + 1;`.
- Typed declaration: `Color c = Color.Green;`.
</details>

<details>
<summary>Exceptions</summary>

```java
int main() {
    try {
        throw 42;
    } catch (int e) {
        printi(e);             // 42
    } finally {
        printi(1);           // always runs, even on throw
    }
    return 0;
}
```

- `try { ... } catch (T e) { ... }`; `throw <value>` raises.
- The catch type may be a primitive (e.g. `int`) or a class error type.
- If the `try` body does not throw, control continues normally.
</details>

<details>
<summary>Objects & memory (escape analysis)</summary>

- Class objects live on the heap with a refcount header.
- Layout: `[refcount:8][field0:8][field1:8]...`.
- Copying a reference increments the refcount (`flint_retain`).
- Dropping a reference decrements it (`flint_release`).
- Escape analysis places class locals that do not escape their scope on the
  stack; only escaping objects (arguments, returns, field stores, `&x`) go to the
  heap with refcounting.
- Primitives are always value types on the stack.
- Cycles leak in v1; there is no weak reference.

```java
class Box {
    int v;

    public Box(int x) {
        this.v = x;
    }
}

int use(Box b) {
    return b.v * 2;
}

int main() {
    Box local = new Box(21);     // does not escape -> stack slot
    printi(local.v);            // 21
    Box keep = new Box(5);
    printi(use(keep));         // 10  (escapes into use -> heap + refcount)
    return 0;
}
```
</details>

<details>
<summary>Control flow</summary>

- `if (c) {} else {}`.
- `while (c) {}`.
- `for (init; cond; update) {}` (all parts optional); `init`/`update` may be
  declarations, assignments, or expressions.
- `for (T x : a)` range-for iterates the elements of array `a` (see Arrays);
  `T x[]` iterates the rows of a multi-dimensional array.
- `break` exits the innermost loop or switch; `continue` jumps to the next loop
  iteration (the `update` clause still runs).
- `switch (e) { case 1: ... break; default: ... }` compares the int value of
  `e` against each case; a case with no `break` falls through to the next case.
- `return` exits the current function.

```java
int main() {
    int x = 0;
    for (int i = 0; i < 5; i = i + 1) {
        if (i == 2) {
            continue;
        }
        if (i == 4) {
            break;
        }
        x += i;
    }
    printi(x);                    // 4  (0 + 1 + 3)
    int i = 0;
    while (i < 3) {
        i = i + 1;
    }
    printi(i);                   // 3
    int a[] = [1, 2, 3];
    int s = 0;
    for (int v : a) {
        s = s + v;
    }
    printi(s);                   // 6  (range-for)
    switch (3) {
        case 2:
            printi(100);
            break;
        case 3:                 // no break: falls through
            printi(300);
        case 4:
            printi(400);
            break;
        default:
            printi(0);
    }
    return 0;
}
```
</details>

<details>
<summary>Operators</summary>

- Arithmetic: `+ - * / %`.
- Bitwise: `& | ^ << >>`.
- Compare: `== != < > <= >=`.
- Unary: `- ! * &` and prefix `++x --x`.
- Postfix: `x++ x-- .field [idx]`.
- `&&` and `||` are logical with short-circuit, returning `0`/`1`.
- `++` and `--` work on lvalues.
- Compound assign: `x += e`, `x -= e`, `x *= e`, `x /= e`.
- Ternary: `c ? a : b` (right-associative, no short-circuit on the branches).
- `/` and `%` truncate toward zero (C semantics): `-7 / 2 == -3`, `-7 % 2 == -1`.

```java
int main() {
    int x = 5;
    int y = 3;
    printi(x + y);                // 8
    printi(x * y);               // 15
    printi(-7 / 2);             // -3
    printi(-7 % 2);             // -1
    printi(1 << 4);             // 16
    printi(40 >> 2);            // 10
    printi(5 & 3);              // 1
    printi(5 | 3);              // 7
    printi(5 ^ 3);              // 6
    printi(2 == 2);             // 1
    printi(!0);                 // 1
    x += 2;
    printi(x++);                // 7  (postfix)
    printi(++x);                // 9  (prefix)
    printi(1 ? 10 : 20);       // 10  (ternary)
    printi(1 || 0);             // 1  (short-circuit)
    printi(0 && 7);             // 0  (short-circuit)
    return 0;
}
```
</details>

<details>
<summary>Pointers & raw memory (escape hatch)</summary>

Low-level access. Standard code should prefer `string` variables plus the
sized `sys.*` builtins; reach for raw pointers only when you need the
`*T`/`&`/`*p` mechanics directly.

- `&x` is address-of; `*p` is dereference.
- `alloc(n)` returns a `*byte` of `n` bytes from the heap (usable through a
  `string` variable too).
- `free(p)` releases a buffer (no-op in v1, reclaimed at exit).
- `buf[i]` reads or writes the 8-byte element at `i` (works on `string` and `*T`).
- Sized access: `sys.byte_load/store`, `sys.short_load/store`,
  `sys.int_load/store` (little-endian) for sub-word cells.

```java
int main() {
    int x = 10;
    *int p = &x;
    *p = 99;
    printi(x);                    // 99  (deref writes through)
    *byte buf = alloc(32);
    buf[0] = 7;
    printi(buf[0]);             // 7
    sys.byte_store(buf + 8, 42);
    printi(sys.byte_load(buf + 8));  // 42  (sized access)
    free(buf);
    return 0;
}
```
</details>

<details>
<summary>Arrays</summary>

- `int a[] = [10, 20, 30];` declares a 1-D array from a literal.
- `int g[][] = [[1, 2], [3, 4]];` is multi-dimensional; repeat `[]`.
- Arrays are heap blocks of 8-byte slots, zero-initialized.
- `a[i]` reads an element; `a[i] = v` writes it; `a[i]++` works.
- `a[i][j]` chains index for multi-dimensional arrays.
- Arrays pass by pointer: `int sum(int a[]) { ... }`.
- Arrays can be class fields: `int data[];` inside a class.
- `int f() { return [7, 8, 9]; }` returns a fresh array.
- Arrays are laid out as `[len:8][elem0:8][elem1:8]...`; `len(a)` returns the
  length in the header (a compile error on a non-array argument).
- `for (int x : a)` range-for desugars to an index loop over `len(a)`.
- No bounds checks and no refcount on elements in v1.

```java
int sum3(int a[]) {
    return a[0] + a[1] + a[2];
}

int main() {
    int a[] = [10, 20, 30];
    a[1] = 25;
    printi(len(a));             // 3
    printi(a[0] + a[1] + a[2]);  // 65
    int g[][] = [[1, 2], [3, 4]];
    printi(g[1][0]);          // 3
    printi(sum3(a));          // 65  (arrays pass by pointer)
    int s = 0;
    for (int v : a) {
        s = s + v;
    }
    printi(s);               // 65
    return 0;
}
```
</details>

<details>
<summary>Collections</summary>

Built-in collection types; one name each, no per-element-type variants. Keys
and values may be ints **or** strings; the compiler infers the flavour from
usage (a string literal, or a value from a `string` variable/call, is stored as a
string, everything else as an int).

- `list`: ordered, `l.add(v)`, `l.get(i)`, `l[i]` / `l[i] = v`, `l.remove(i)`,
  `l.contains(v)`, `l.size()`, `l.clear()`, `len(l)`.
- `queue`: FIFO, `q.push(v)`, `q.pop()`, `q.peek()`, `q.size()`, `q.clear()`,
  `len(q)`.
- `hashmap`: `m.put(k, v)`, `m.get(k)`, `m.contains(k)`, `m.remove(k)`,
  `m.size()`, `m.clear()`, `m.keys()` / `m.values()` (fresh `list`), `len(m)`.
- `hashset`: `s.add(v)`, `s.contains(v)`, `s.remove(v)`, `s.size()`,
  `s.clear()`, `len(s)`.
- `dictionary` is an alias of `hashmap`.
- Typed forms: `list<int>`, `list<string>`, `hashmap<string, int>`,
  `hashset<int>`, `queue<int>` (the untyped forms still work).
- Construction: `list l = new list();`, `queue q = new queue(1, 2, 3);`,
  `list l = [1, 2, 3];`, `hashset s = {3, 4};`, `hashmap m = {1: "a", 2: "b"};`
  (an empty `{}` reads as a map; a `{...}` literal outside a collection
  declaration is a compile error).
- Collections are heap objects (slot 0 = size, 8-byte slots, power-of-two caps)
  and are not refcounted; they pass by pointer and can be class fields.
- `get`/`pop`/`peek`/`l[i]` return an int or a string depending on the declared
  type of the receiving variable or the function return type; the flavour must
  match what was stored, otherwise 0 is returned.
- `contains`/`remove` match on the value's flavour: `m.contains("a")` looks for a
  string key, `m.contains(1)` for an int key.

```java
int main() {
    list l = [10, 20, 30];
    l.add(40);
    l.sort();
    l.reverse();
    print(l.join("-"));
    print("\n");                  // 40-30-20-10
    hashmap m = new hashmap();
    m.put("apple", 1);
    m.put("banana", 2);
    printi(m.get("banana"));     // 2
    printi(len(m));             // 2
    hashset s = new hashset();
    s.add(5);
    s.add(5);                  // sets discard duplicates
    printi(len(s));            // 1
    queue q = new queue();
    q.push(1);
    q.push(2);
    printi(q.pop());           // 1  (FIFO)
    return 0;
}
```
</details>

<details>
<summary>Generics</summary>

Generic classes and functions, monomorphized at compile time. Each
`(template, type-args)` pair expands to a concrete type/function with a mangled
name (`Vessel_int`, `identity_int`); there is no runtime type erasure.

- Generic class: `class Vessel<T> { T v; }`, used as
  `Vessel<int> b = new Vessel<int>(5);`.
- Generic function: `T identity<T>(T x) { return x; }`, called as
  `identity(5)` (type args inferred from the arguments) or
  `identity<int>(5)` (explicit).
- Typed collections: `list<int>`, `list<string>`, `hashmap<string, int>`,
  `hashset<int>`, `queue<int>`; the untyped forms above still work.
- Nested instantiations are expanded to a fixpoint, so
  `Vessel<Pair<int, int>>` works.

```java
class Vessel<T> {
    T v;
}

T identity<T>(T x) {
    return x;
}

int main() {
    Vessel<int> a = new Vessel<int>(41);
    printi(a.v);                // 41
    printi(identity(7));        // 7  (type inferred from the argument)
    list<int> l = new list<int>();
    l.add(5);
    printi(len(l));           // 1  (typed collection)
    return 0;
}
```
</details>

<details>
<summary>Strings</summary>

- String literals are NUL-terminated `string`.
- A literal stored into a `string` variable/field/param/return is a fresh
  writable, refcounted copy: `string s = ""; s[0] = 65;` works (no `alloc`,
  no segfault), and the literal itself is never modified.
- `(string)N`: a fresh single-character string for the byte value `N`
  (`"hi" + (string)72` → `"hiH"`); `string + int` stays pointer arithmetic.
- `print(s)` writes a string to stdout.
- `strlen(s)` returns its byte length.
- `str.concat(a, b)`: a fresh NUL-terminated copy of `a` followed by `b`.
- `str.concati(s, n)`: string + int (decimal) as a fresh string.
- `str.itoa(n)`: a fresh decimal string for an int; `conv.atoi(s)` parses it back.
- `str.cmp(a, b)`: byte-wise, like `strcmp`: `< 0`, `0`, or `> 0`.
- `str.copy(s)`: a fresh NUL-terminated copy of `s`.
- `str.substring(s, from, len)`: a fresh slice of `s`.
- `str.indexOf(s, sub)`: first index of `sub` in `s` (or -1).
- `str.replace(s, from, to)`: replace an occurrence of `from` with `to`.
- `str.format(fmt, a, b, c, d)`: printf-style with `%d`, `%s`, `%b` placeholders.
- Strings are passed and stored by pointer.

```java
int main() {
    string s = "hello" + " world";
    print(s);
    print("\n");                        // hello world
    printi(str.cmp("hello", "hello") == 0 ? 1 : 0);   // 1
    string sub = str.substring("hello world", 6, 5);
    printi(str.cmp(sub, "world") == 0 ? 1 : 0);       // 1
    printi(str.indexOf("hello world", "world") == 6 ? 1 : 0);  // 1
    string r = str.replace("aaa", "a", "b");
    printi(str.cmp(r, "bbb") == 0 ? 1 : 0);          // 1
    string f = str.format("%d + %d = %d", 1, 2, 3, 0);
    printi(str.cmp(f, "1 + 2 = 3") == 0 ? 1 : 0);   // 1
    printi(conv.atoi(str.itoa(123)) == 123 ? 1 : 0);  // 1
    return 0;
}
```
</details>

<details>
<summary>Floats (fixed-point)</summary>

Floats are Q48.16 fixed-point: a 64-bit int whose lower 16 bits are the
fraction. `0.5` is `32768`. Provided by the `math.*` builtins:

- `math.itof(n)`: int to float; `math.ftoi(f)`: float to int.
- `math.fadd`, `math.fsub`, `math.fmul`, `math.fdiv`: arithmetic.
- `math.fcmp(a, b)`: returns `< 0`, `0`, or `> 0`.

```java
int main() {
    int a = math.itof(3);
    int b = math.itof(2);
    int c = math.fadd(a, b);   // 5.0
    printi(math.ftoi(c));       // 5
    return 0;
}
```
</details>

<details>
<summary>Concurrency</summary>

```java
void thread_fn(int arg) {
    sys.write(1, "hello from thread", 19);
}
int main() {
    int tid = sys.thread_create(@thread_fn, 0);  // @ = function address
    sys.thread_join(tid);
    return 0;
}
```

- `sys.thread_create(@fn, arg)` starts a thread; `sys.thread_join(tid)` waits.
- The `@` operator yields a function's address (required to launch a thread).
- `sys.mutex_lock(&m)` / `sys.mutex_unlock(&m)`: a simple mutex.
- `sys.atomic_cas(&v, old, new)`: compare-and-swap; returns 1 on success.
- Lower-level: `sys.clone`, `sys.futex`, `sys.nanosleep(sec, nsec)`.
</details>

<details>
<summary>Networking (TCP)</summary>

```java
int main() {
    int s = sys.socket(2, 1, 0);           // AF_INET, SOCK_STREAM
    sys.bind(s, sys.sockaddr(7777, 127, 0, 0, 1), 16);
    sys.listen(s, 5);
    int c = sys.accept(s, 0, 0);           // or: sys.connect(s, sys.sockaddr(7777, 127, 0, 0, 1), 16)
    string buf = alloc(64);
    int n = sys.read(c, buf, 64);
    sys.write(c, buf, n);                 // echo
    sys.close(c);
    return 0;
}
```

- `sys.socket(fam, type, proto)`: create a socket (`AF_INET=2`, `SOCK_STREAM=1`).
- `sys.sockaddr(port, o1, o2, o3, o4)`: build a 16-byte `sockaddr_in`.
- `sys.bind`, `sys.listen`, `sys.accept` (server); `sys.connect` (client).
- Read/write over a socket fd; `sys.close` to release.
- Multiplexing: `sys.select`, `sys.poll`, `sys.epoll_create1/ctl/wait`.
</details>

<details>
<summary>I/O & system</summary>

- File I/O: `sys.open(path, flags, mode)` -> fd, `sys.read/write(fd, buf, n)`,
  `sys.close(fd)`.
- Standard streams: `print(s)`, `printi(n)`; stdin via `sys.read(0, buf, n)`.
- `sys.exit(code)` terminates the process.
- `env.get("NAME")`: an environment variable (a `string`, or `null` if unset).
- `time.millis()`: wall-clock milliseconds.
- `rand.next()`: a 64-bit draw; `rand.range(min, max)`: in `[min, max)`.
- `log.info/warn/error/debug(s)`: levelled log output.
- `b64.encode(s)` / `b64.decode(s)`: base64 round-trip.
- `json.get(s, key)` / `json.geti(s, key)`: key extraction from a flat JSON object.
- `sys.brk(addr)`: move the program break.
- `sys.syscall(num, a1, a2, a3, a4, a5)`: a generic raw-syscall escape hatch.

```java
int main() {
    int f = sys.open("/tmp/flint_io.txt", 0x241, 0x1a4);  // O_RDWR|O_CREAT|O_TRUNC
    sys.write(f, "hi", 2);
    sys.close(f);
    string p = env.get("PATH");
    printi(p != null ? 1 : 0);           // 1
    printi(time.millis() > 1000000000000 ? 1 : 0);  // 1
    int r = rand.range(10, 20);
    printi(r >= 10 && r < 20 ? 1 : 0);   // 1
    string e = b64.encode("hello");
    printi(str.cmp(e, "aGVsbG8=") == 0 ? 1 : 0);  // 1
    log.info("hello");                  // [INFO] hello
    return 0;
}
```
</details>

<details>
<summary>Modules (package & import)</summary>

```java
// util.flint
package com.example;

class Util {
    public Util() {
    }

    public int doubleIt(int x) {
        return x * 2;
    }
}

// main.flint
import com.example.Util;

int main() {
    Util u = new Util();
    printi(u.doubleIt(21));
    return 0;
}
```

- `package com.example;` declares the package for a file (or its declarations).
- `import com.a.A;` pulls a type from another package into scope; the import
  resolves by the last path segment, so `import std.Math;` makes `Math`
  available and `import std.*;` pulls in every type of `std`.
- An imported name only resolves when it is actually a class/interface/enum of
  the imported package.
- Static members go through the class name: `Math.abs(7)`, `Counter.n`.
- Compile multiple files together, or via a TOML manifest / a directory; each
  file keeps its own `package` header.
</details>

<details>
<summary>Comments</summary>

```java
// line comment
/* block
   comment */
int main() {
    // printi(1);  commented out
    return 0;
}
```

- `// line` and `/* block */`.
</details>

## Standard library (builtins)

Two layers:

1. **Builtins** (this section): compiler-provided, backed by `intrinsics.s`
   and `collections.s`, documented by the runnable reference modules in
   `src/prelude/`. Always available.
2. **`std.*` modules**: a standard library written entirely in Flint
   (`package std`, in `src/stdlib/`): `std.Math`, `std.Sort`, `std.Bit`,
    `std.Num`, `std.File`, `std.Path`, `std.Checksum`, `std.Rand`,
    `std.Str`, `std.Time`. Compile
   `src/stdlib/*.flint` alongside your sources and use
   `import std.X;` + `X.method(...)` (see the `std.*` feature section for
   the per-class method lists).

Builtins, grouped by module:

| module | functions |
|--------|-----------|
| `sys`  | files: `open` `read` `write` `close` `brk`; process: `exit` `syscall`; sockets: `socket` `bind` `listen` `accept` `connect` `sockaddr`; multiplexing: `select` `poll` `epoll_create1` `epoll_ctl` `epoll_wait`; threads: `clone` `futex` `thread_create` `thread_join` `nanosleep`; locks: `mutex_lock` `mutex_unlock` `atomic_cas`; memory: `byte_load` `byte_store` `short_load` `short_store` `int_load` `int_store` |
| `io`   | `print` `printi` (stdin via `sys.read(0, buf, n)`) |
| `mem`  | `alloc` `free` `memcpy` `strlen` |
| `str`  | `strlen` `concat` `concati` `itoa` `cmp` `copy` `substring` `indexOf` `replace` `format` |
| `conv` | `atoi` |
| `math` | `itof` `ftoi` `fadd` `fsub` `fmul` `fdiv` `fcmp` (Q48.16 fixed-point) |
| `time` | `millis` |
| `rand` | `next` `range` |
| `env`  | `get` |
| `b64`  | `encode` `decode` |
| `json` | `get` `geti` |
| `log`  | `info` `warn` `error` `debug` |
| `len`  | `len` (array / collection size) |

Examples (each block is a complete, runnable `main`):

```java
// Text, numbers, and fixed-point (str / conv / math / len)
int main() {
    string s = "hello" + " world";
    printi(strlen(s));                                // 11
    printi(str.indexOf(s, "world"));                // 6
    printi(str.cmp("abc", "abc") == 0 ? 1 : 0);     // 1
    printi(conv.atoi(str.itoa(123)) == 123 ? 1 : 0);  // 1
    string f = str.format("%d + %d = %d", 1, 2, 3, 0);
    print(f);                                     // 1 + 2 = 3
    print("\n");
    int a[] = [1, 2, 3];
    printi(len(a));                                 // 3
    printi(math.ftoi(math.fadd(math.itof(3), math.itof(2))));  // 5
    return 0;
}
```

```java
// Clock, randomness, environment, encoding, json, logging
int main() {
    printi(time.millis() > 1000000000000 ? 1 : 0);   // 1
    printi(rand.range(10, 20) >= 10 ? 1 : 0);        // 1
    string p = env.get("PATH");
    printi(p != null ? 1 : 0);                         // 1 if PATH is set
    string e = b64.encode("hello");
    printi(str.cmp(e, "aGVsbG8=") == 0 ? 1 : 0);      // 1
    printi(json.geti("{\"n\": 7}", "n") == 7 ? 1 : 0);  // 1
    log.info("hello");
    return 0;
}
```

```java
// Files and sized memory access (sys / mem)
int main() {
    int fd = sys.open("/tmp/flint_builtin.txt", 0x241, 0x1a4);
    sys.write(fd, "hi", 2);
    sys.close(fd);
    string buf = alloc(16);
    sys.byte_store(buf + 8, 65);
    printi(sys.byte_load(buf + 8));                   // 65
    sys.short_store(buf, 1000);
    printi(sys.short_load(buf));                     // 1000
    free(buf);
    return 0;
}
```

Syscall convention: `rax` is the number, args are `rdi rsi rdx r10 r8 r9`
(4th is `r10`), `syscall` clobbers `rcx r11`.

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
   stdlib/   bit.flint  checksum.flint  file.flint  math.flint
             num.flint  path.flint  rand.flint  sort.flint
             str.flint  time.flint
  intrinsics.s  collections.s
tests/  flintc.rs  run_tests.sh  cases/  golden/  errors/  multifile/  thread_*.flint
examples/  hello.flint  fib.flint  file_copy.flint  alloc_demo.flint
            tcp_client.flint  tcp_server.flint
```
