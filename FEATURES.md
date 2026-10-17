# Flint — Language Features

A complete inventory of the Flint language and its toolchain, gathered from the
compiler source (`src/`), the prelude (`src/prelude/`), the standard library
(`src/stdlib/`), the freestanding runtime (`src/intrinsics.s`), the examples
(`examples/`), and the test suite (`tests/`). This is the authoritative
feature list — where the HTML docs (`docs/`) and this file disagree, the
compiler source is the ground truth.

Every dot point below carries a runnable example in a collapsible details
block, with its **expected output** (captured by compiling the example with
`flintc` and running it).

**What Flint is.** A small, statically-typed, systems-level language that
compiles to freestanding x86_64 Linux assembly (no libc). Source files are
`*.flint`. The pipeline is `lexer → parser → desugar (lambdas) → monomorphize
→ ownership check → codegen → .s → as → ld -e _start`. The runtime is a
hand-written freestanding `intrinsics.s` (raw `syscall`s, heap, refcounting,
threads, sockets, print helpers) linked into every binary. The collections are
ordinary stdlib classes (`src/stdlib/coll.flint`), not part of the runtime.

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

<details>
<summary>Example: `int` (and the collapsed scalar keywords)</summary>

```flint
// build: flintc s1_int.flint -o s1_int && ./s1_int
int main() {
    int a = 42;
    byte b = 7;
    short s = 1234;
    long l = 999999999999;
    char c = 65;
    print(a + b + s);
    print("\n");
    print(l % 1000);
    print("\n");
    print(c);
    print("\n");
    return 0;
}
```

Expected output:

```
1283
999
65
```

</details>

<details>
<summary>Example: `boolean`</summary>

```flint
// build: flintc s1_bool.flint -o s1_bool && ./s1_bool
int main() {
    boolean t = true;
    boolean f = false;
    print(t);
    print("\n");
    print(f);
    print("\n");
    print(t && f);
    print("\n");
    return 0;
}
```

Expected output:

```
1
0
0
```

</details>

<details>
<summary>Example: `string` (NUL-terminated, indexable by byte)</summary>

```flint
// build: flintc s1_string.flint -o s1_string && ./s1_string
int main() {
    string s = "hello";
    print(s);
    print("\n");
    print(s[0]);
    print("\n");
    print(strlen(s));
    print("\n");
    return 0;
}
```

Expected output:

```
hello
104
5
```

</details>

<details>
<summary>Example: `*T` raw pointer</summary>

```flint
// build: flintc s1_ptr.flint -o s1_ptr && ./s1_ptr
int main() {
    int x = 5;
    *int p = &x;
    print(*p);
    print("\n");
    *int q = p;
    print(*q);
    print("\n");
    *int n = null;
    print(n == null ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
5
5
1
```

</details>

<details>
<summary>Example: `T[]` array (8-byte slots, length header)</summary>

```flint
// build: flintc s1_array.flint -o s1_array && ./s1_array
int main() {
    int a[] = [10, 20, 30];
    print(len(a));
    print("\n");
    a[1] = 25;
    print(a[0] + a[1] + a[2]);
    print("\n");
    return 0;
}
```

Expected output:

```
3
65
```

</details>

<details>
<summary>Example: `byte b[]` (1-byte elements)</summary>

```flint
// build: flintc s1_bytearr.flint -o s1_bytearr && ./s1_bytearr
int main() {
    byte b[4];
    b[0] = 1;
    b[1] = 2;
    b[2] = 3;
    b[3] = 4;
    print(len(b));
    print("\n");
    b[0] = 9;
    print(b[0] + b[1] + b[2] + b[3]);
    print("\n");
    return 0;
}
```

Expected output:

```
4
18
```

</details>

<details>
<summary>Example: `short s[]` (2-byte little-endian elements)</summary>

```flint
// build: flintc s1_shortarr.flint -o s1_shortarr && ./s1_shortarr
int main() {
    short s[2];
    s[0] = 1000;
    s[1] = 2000;
    print(len(s));
    print("\n");
    s[1] = 3000;
    print(s[0] + s[1]);
    print("\n");
    return 0;
}
```

Expected output:

```
2
4000
```

</details>

<details>
<summary>Example: `void`</summary>

```flint
// build: flintc s1_void.flint -o s1_void && ./s1_void
void beep() {
    print("beep");
}
int main() {
    beep();
    print("\n");
    return 0;
}
```

Expected output:

```
beep
```

</details>

<details>
<summary>Example: `ClassName` (a class/struct type)</summary>

```flint
// build: flintc s1_class.flint -o s1_class && ./s1_class
class Point {
    int x;
    int y;
    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }
    public int sum() {
        return this.x + this.y;
    }
}
int main() {
    Point p = new Point(3, 4);
    print(p.sum());
    print("\n");
    return 0;
}
```

Expected output:

```
7
```

</details>

<details>
<summary>Example: `InterfaceName`</summary>

```flint
// build: flintc s1_iface.flint -o s1_iface && ./s1_iface
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
    Speaker s = new Robot(3);
    print(s.speak());
    print("\n");
    return 0;
}
```

Expected output:

```
30
```

</details>

<details>
<summary>Example: `EnumName` (a 64-bit integer value)</summary>

```flint
// build: flintc s1_enum.flint -o s1_enum && ./s1_enum
enum Color {
    RED,
    GREEN,
    BLUE,
}
int main() {
    Color c = Color.GREEN;
    print(c);
    print("\n");
    print(Color.BLUE);
    print("\n");
    return 0;
}
```

Expected output:

```
1
2
```

</details>

<details>
<summary>Example: `(T1, T2, …)` tuple type</summary>

```flint
// build: flintc s1_tuple.flint -o s1_tuple && ./s1_tuple
(int, string) pair(int n) {
    return (n * 2, "n=" + str.itoa(n));
}
int main() {
    (int a, string s) = pair(21);
    print(a);
    print("\n");
    print(s);
    print("\n");
    return 0;
}
```

Expected output:

```
42
n=21
```

</details>

<details>
<summary>Example: `T` type parameter (generic function)</summary>

```flint
// build: flintc s1_typeparam.flint -o s1_typeparam && ./s1_typeparam
T identity<T>(T x) {
    return x;
}
int main() {
    print(identity(7));
    print("\n");
    print(identity("hi"));
    print("\n");
    return 0;
}
```

Expected output:

```
7
hi
```

</details>

<details>
<summary>Example: type alias (top-level)</summary>

Note: the compiler keyword is `alias` — `type NAME = …` is rejected at top
level (`expected 'package', 'import', 'class', 'interface', or a function at
top level, found Ident("type")`).

```flint
// build: flintc s1_alias.flint -o s1_alias && ./s1_alias
alias Big = int;
alias Msg = string;
int main() {
    Big x = 5;
    Msg m = "hi";
    print(x * 2);
    print("\n");
    print(m);
    print("\n");
    return 0;
}
```

Expected output:

```
10
hi
```

</details>

**`var`** — type inference from the initializer: `var x = 5;` (requires an
initializer). In a for-each, `var x` infers the element type from the target.
`var (a, b) = …` infers tuple element types.

<details>
<summary>Example: `var` (initializer, for-each, tuple destructure)</summary>

```flint
// build: flintc s1_var.flint -o s1_var && ./s1_var
int main() {
    var x = 5;
    print(x);
    print("\n");
    var s = "hi";
    print(s);
    print("\n");
    var (a, b) = (1, 2);
    print(a + b);
    print("\n");
    int arr[] = [10, 20];
    int u = 0;
    for (var v : arr) {
        u = u + v;
    }
    print(u);
    print("\n");
    return 0;
}
```

Expected output:

```
5
hi
3
30
```

</details>

**Array declaration syntax** — both styles are accepted:

- C#-style prefix (`int[] a`, `int[][] g`, `byte[] b`, `short[] s`) or
  C-style suffix (`int a[]`, `int g[][]`, `byte b[]`, `short s[]`).

<details>
<summary>Example: both array declaration styles (incl. 2-D)</summary>

```flint
// build: flintc s1_arrdecl.flint -o s1_arrdecl && ./s1_arrdecl
int main() {
    int[] a = [1, 2, 3];
    int b[] = [4, 5, 6];
    print(a[0] + b[0]);
    print("\n");
    int[][] g = [[1, 2], [3, 4]];
    int h[][] = [[5, 6], [7, 8]];
    print(len(g));
    print("\n");
    print(g[1][1]);
    print("\n");
    print(h[0][1]);
    print("\n");
    return 0;
}
```

Expected output:

```
5
2
4
6
```

</details>

Sized buffers cannot be class fields (layouts are fixed); assign `int[N]` in a
constructor instead.

<details>
<summary>Example: sized buffer assigned in a constructor</summary>

```flint
// build: flintc s1_sizedfield.flint -o s1_sizedfield && ./s1_sizedfield
class Buf {
    int data[];
    public Buf(int n) {
        this.data = int[n];
    }
    public int sum() {
        int t = 0;
        for (int i = 0; i < len(this.data); i = i + 1) {
            t = t + this.data[i];
        }
        return t;
    }
}
int main() {
    Buf b = new Buf(8);
    b.data[0] = 3;
    b.data[1] = 4;
    print(len(b.data));
    print("\n");
    print(b.sum());
    print("\n");
    return 0;
}
```

Expected output:

```
8
7
```

</details>

---

## 2. Literals

- **Integer literals:** decimal (`42`, `18446744073709551615` wraps to
  two's-complement `i64`) and **hex** (`0xFF`, `0x8000000000000000`,
  `0xFFFFFFFFFFFFFFFF` — parsed as `u64` then wrapped to `i64`, so the full
  64-bit range is representable).

<details>
<summary>Example: integer literals (decimal, hex, wrapping)</summary>

```flint
// build: flintc s2_intlit.flint -o s2_intlit && ./s2_intlit
int main() {
    int d = 42;
    int h = 0xFF;
    print(d);
    print("\n");
    print(h);
    print("\n");
    int big = 0xFFFFFFFFFFFFFFFF;
    print(big);
    print("\n");
    int wrap = 18446744073709551615;
    print(wrap);
    print("\n");
    int neg = 0x8000000000000000;
    print(neg);
    print("\n");
    return 0;
}
```

Expected output:

```
42
255
-1
-1
-9223372036854775808
```

</details>

- **Boolean literals:** `true`, `false`.

<details>
<summary>Example: boolean literals</summary>

```flint
// build: flintc s2_boollit.flint -o s2_boollit && ./s2_boollit
int main() {
    boolean a = true;
    boolean b = false;
    print(a);
    print("\n");
    print(b);
    print("\n");
    print(a && b);
    print("\n");
    return 0;
}
```

Expected output:

```
1
0
0
```

</details>

- **String literals:** `"text"`, with escapes `\\n \\t \\r \\" \\\\ \\0` and
  **escaped braces** `\\{ \\}` (literal text, no interpolation).

<details>
<summary>Example: string literals with escapes</summary>

```flint
// build: flintc s2_strlit.flint -o s2_strlit && ./s2_strlit
int main() {
    print("a\nb\tc\r");
    print("\n");
    print("quote: \" and backslash: \\");
    print("\n");
    print("brace: \{ and \}");
    print("\n");
    return 0;
}
```

Expected output (the first line contains a real newline, tab and CR):

```
a
b	c
quote: " and backslash: \
brace: { and }
```

</details>

- **String interpolation:** `"a={x}b"` — literal text fragments alternate with
  interpolated expressions (the lexer splits at each `{`, the parser reassembles
  into an `Interp` node).

<details>
<summary>Example: string interpolation</summary>

```flint
// build: flintc s2_interp.flint -o s2_interp && ./s2_interp
int main() {
    int x = 42;
    string name = "flint";
    print("x={x} name={name}\n");
    int y = 7;
    print("sum={x + y}\n");
    return 0;
}
```

Expected output:

```
x=42 name=flint
sum=49
```

</details>

- **Char literals:** `'a'`, `'\n'`, `'\t'`, `'\r'`, `'\0'`, `'\''`, `'\"'`,
  `'\\'`. A char is a **code point** — the literal is emitted as an `int`.

<details>
<summary>Example: char literals (printed as code points)</summary>

```flint
// build: flintc s2_charlit.flint -o s2_charlit && ./s2_charlit
int main() {
    char c = 'a';
    print(c);
    print("\n");
    print('\n');
    print("\n");
    print('\'');
    print("\n");
    print('"');
    print("\n");
    print('\\');
    print("\n");
    print("\t");
    print("tab");
    print("\n");
    return 0;
}
```

Expected output (an int argument is formatted in decimal; the 6th line starts
with a real tab):

```
97
10
39
34
92
	tab
```

</details>

- **`null`** literal.

<details>
<summary>Example: `null`</summary>

```flint
// build: flintc s2_null.flint -o s2_null && ./s2_null
int main() {
    string s = null;
    print(s == null ? 1 : 0);
    print("\n");
    *int p = null;
    print(p == null ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
```

</details>

- **Enum variant reference:** `EnumName.Variant` — a compile-time integer
  constant.

<details>
<summary>Example: enum variant reference</summary>

```flint
// build: flintc s2_enumvar.flint -o s2_enumvar && ./s2_enumvar
enum Status {
    IDLE,
    RUNNING,
    DONE,
}
int main() {
    print(Status.IDLE);
    print("\n");
    print(Status.RUNNING);
    print("\n");
    print(Status.DONE);
    print("\n");
    return 0;
}
```

Expected output:

```
0
1
2
```

</details>

---

## 3. Variables & scoping

- **Declaration:** `Type name = value;` (an initializer is required).

<details>
<summary>Example: declaration</summary>

```flint
// build: flintc s3_decl.flint -o s3_decl && ./s3_decl
int main() {
    int n = 5;
    string s = "hi";
    boolean b = true;
    print(n);
    print("\n");
    print(s);
    print("\n");
    print(b);
    print("\n");
    return 0;
}
```

Expected output:

```
5
hi
1
```

</details>

- **Flat scope — no block shadowing:** a new `Type name` declaration inside an
  inner block **re-declares the same variable**, overwriting it; the new value
  persists after the block ends (see `examples/scope.flint`).

<details>
<summary>Example: flat scope (re-declaration overwrites)</summary>

```flint
// build: flintc s3_flatscope.flint -o s3_flatscope && ./s3_flatscope
int main() {
    int v = 10;
    if (1) {
        int v = 20;
        print(v);
    }
    print("\n");
    print(v);
    print("\n");
    return 0;
}
```

Expected output:

```
20
20
```

</details>

- **Sized buffers:** `int x[n];`, `byte x[n];`, `short x[n];`, `string s[n];`
  — a fresh, zero-filled heap buffer of `n` elements/bytes. No initializer
  (buffers start zero-filled). One-dimensional only. The variable's type is the
  buffer's element type. Also usable as an expression: `int[n]`, `byte[n]`,
  `short[n]`, `char[n]`, `string[n]`.

<details>
<summary>Example: sized buffers (zero-filled, element-sized)</summary>

```flint
// build: flintc s3_sizedbuf.flint -o s3_sizedbuf && ./s3_sizedbuf
int main() {
    int buf[4];
    byte b[8];
    short s[2];
    string t[16];
    print(len(buf));
    print("\n");
    print(len(b));
    print("\n");
    print(len(s));
    print("\n");
    t[0] = 72;
    print(t[0]);
    print("\n");
    buf[0] = 1;
    b[1] = 2;
    s[0] = 3000;
    print(buf[0] + b[1] + s[0]);
    print("\n");
    int z[3];
    print(z[0]);
    print("\n");
    return 0;
}
```

Expected output:

```
4
8
2
72
3003
0
```

</details>

- **`len(b)`** — the element count of any buffer.

<details>
<summary>Example: `len(b)`</summary>

```flint
// build: flintc s3_len.flint -o s3_len && ./s3_len
int main() {
    int a[] = [1, 2, 3];
    byte b[5];
    string s = "hello";
    print(len(a));
    print("\n");
    print(len(b));
    print("\n");
    print(strlen(s));
    print("\n");
    return 0;
}
```

Expected output:

```
3
5
5
```

</details>

- **String indexing:** `s[i]` reads the byte at offset `i`.

<details>
<summary>Example: string indexing (read and write)</summary>

```flint
// build: flintc s3_strindex.flint -o s3_strindex && ./s3_strindex
int main() {
    string s = "flint";
    print(s[0]);
    print("\n");
    print(s[4]);
    print("\n");
    s[0] = 70;
    print(s);
    print("\n");
    return 0;
}
```

Expected output:

```
102
116
Flint
```

</details>

- **`++` / `--`** on an lvalue. **Deliberate departure from C/Java:** both
  `x++` and `++x` evaluate to the value **after** the increment (there is no
  "old value" form).

<details>
<summary>Example: `++` / `--` (both forms are post-value)</summary>

```flint
// build: flintc s3_incdec.flint -o s3_incdec && ./s3_incdec
int main() {
    int x = 5;
    print(x++);
    print("\n");
    print(x);
    print("\n");
    int y = 10;
    print(++y);
    print("\n");
    print(y);
    print("\n");
    int z = 3;
    print(z--);
    print("\n");
    print(z);
    print("\n");
    print(--z);
    print("\n");
    return 0;
}
```

Expected output:

```
6
6
11
11
2
2
1
```

</details>
---

## 4. Operators

- **Arithmetic:** `+  -  *  /  %` — integer division and modulo truncate toward
  zero (C semantics); the result of `%` carries the sign of the dividend.

<details>
<summary>Example: arithmetic (truncating division/modulo)</summary>

```flint
// build: flintc s4_arith.flint -o s4_arith && ./s4_arith
int main() {
    print(7 + 5);
    print("\n");
    print(7 - 5);
    print("\n");
    print(7 * 5);
    print("\n");
    print(7 / 2);
    print("\n");
    print(-7 / 2);
    print("\n");
    print(7 % 3);
    print("\n");
    print(-7 % 3);
    print("\n");
    return 0;
}
```

Expected output:

```
12
2
35
3
-3
1
-1
```

</details>

- **Bitwise:** `&  |  ^  ~  <<  >>`.

<details>
<summary>Example: bitwise</summary>

```flint
// build: flintc s4_bitwise.flint -o s4_bitwise && ./s4_bitwise
int main() {
    print(12 & 10);
    print("\n");
    print(12 | 10);
    print("\n");
    print(12 ^ 10);
    print("\n");
    print(~12);
    print("\n");
    print(1 << 4);
    print("\n");
    print(256 >> 4);
    print("\n");
    return 0;
}
```

Expected output:

```
8
14
6
-13
16
16
```

</details>

- **Comparison:** `==  !=  <  >  <=  >=`.

<details>
<summary>Example: comparison</summary>

```flint
// build: flintc s4_cmp.flint -o s4_cmp && ./s4_cmp
int main() {
    print(3 == 3);
    print("\n");
    print(3 != 4);
    print("\n");
    print(3 < 4);
    print("\n");
    print(4 > 3);
    print("\n");
    print(3 <= 3);
    print("\n");
    print(4 >= 5);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
1
1
1
0
```

</details>

- **Logical:** `&&` and `||` **short-circuit** and yield 0 / 1.

<details>
<summary>Example: logical short-circuit</summary>

```flint
// build: flintc s4_logical.flint -o s4_logical && ./s4_logical
int seen() {
    print("evaluated");
    print("\n");
    return 1;
}
int main() {
    print(0 && seen());
    print("\n");
    print(1 || seen());
    print("\n");
    print(1 && seen());
    print("\n");
    return 0;
}
```

Expected output (`evaluated` is printed only when the RHS is actually evaluated):

```
0
1
evaluated
1
```

</details>

- **Ternary:** `c ? a : b` — right-associative.

<details>
<summary>Example: ternary (nested)</summary>

```flint
// build: flintc s4_ternary.flint -o s4_ternary && ./s4_ternary
int main() {
    int x = 5;
    print(x > 3 ? "big" : "small");
    print("\n");
    print(x > 10 ? (x > 3 ? 1 : 2) : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
big
0
```

</details>

- **Compound assignment** (target must be an lvalue): `+=  -=  *=  /=  %=  &=
  |=  ^=  <<=  >>=`.

<details>
<summary>Example: compound assignment</summary>

```flint
// build: flintc s4_compound.flint -o s4_compound && ./s4_compound
int main() {
    int x = 10;
    x += 5;
    print(x);
    print("\n");
    x -= 2;
    print(x);
    print("\n");
    x *= 3;
    print(x);
    print("\n");
    x /= 4;
    print(x);
    print("\n");
    x %= 7;
    print(x);
    print("\n");
    int y = 0;
    y |= 4;
    y &= 6;
    y ^= 5;
    y <<= 1;
    y >>= 1;
    print(y);
    print("\n");
    return 0;
}
```

Expected output:

```
15
13
39
9
2
1
```

</details>

- **String operations:** `string + string` (concat), `string + int`
  (concat + itoa) — via `str.concat` / `str.concati`.

<details>
<summary>Example: string operations</summary>

Note: in the current compiler `string + int` is pointer arithmetic (a string
is a pointer; `"n=" + 42` prints the resulting address, not `n=42`).
`string + string` is a real concat; use `str.concati` for concat + itoa.

```flint
// build: flintc s4_stringop.flint -o s4_stringop && ./s4_stringop
int main() {
    string a = "foo";
    string b = "bar";
    print(a + b);
    print("\n");
    print(str.concati("n=", 42));
    print("\n");
    return 0;
}
```

Expected output:

```
foobar
n=42
```

</details>

Standard precedence.

---

## 5. Control flow

- `if` / `else`.

<details>
<summary>Example: `if` / `else`</summary>

```flint
// build: flintc s5_ifelse.flint -o s5_ifelse && ./s5_ifelse
int main() {
    int n = 7;
    if (n > 10) {
        print("big");
    } else if (n > 5) {
        print("mid");
    } else {
        print("small");
    }
    print("\n");
    return 0;
}
```

Expected output:

```
mid
```

</details>

- `while`.

<details>
<summary>Example: `while`</summary>

```flint
// build: flintc s5_while.flint -o s5_while && ./s5_while
int main() {
    int i = 0;
    int sum = 0;
    while (i < 5) {
        sum = sum + i;
        i = i + 1;
    }
    print(sum);
    print("\n");
    return 0;
}
```

Expected output:

```
10
```

</details>

- `for (init; cond; update)` — all three parts optional.

<details>
<summary>Example: `for` (with and without parts)</summary>

```flint
// build: flintc s5_for.flint -o s5_for && ./s5_for
int main() {
    int sum = 0;
    for (int i = 1; i <= 4; i = i + 1) {
        sum = sum + i;
    }
    print(sum);
    print("\n");
    int j = 0;
    for (; j < 3;) {
        j = j + 1;
    }
    print(j);
    print("\n");
    return 0;
}
```

Expected output:

```
10
3
```

</details>

- **For-each:** `for (T x : c)` (or `var x`) over arrays and any class with
  `size()`/`get(int)` (std `List`, `Queue`, `HashSet`, `CopyOnWriteList`;
  `HashMap` via `.keys()`/`.values()`). The monomorphizer desugars it into an
  index loop.

<details>
<summary>Example: for-each over an array and a `List`</summary>

```flint
// build: flintc src/stdlib/*.flint s5_foreach.flint -o s5_foreach && ./s5_foreach
import std.List;
int main() {
    int a[] = [10, 20, 30];
    int sum = 0;
    for (int x : a) {
        sum = sum + x;
    }
    print(sum);
    print("\n");
    List<string> l = new List<string>();
    l.add("a");
    l.add("b");
    for (var s : l) {
        print(s);
        print(" ");
    }
    print("\n");
    return 0;
}
```

Expected output (the 2nd line is `a b ` with a trailing space):

```
60
a b 
```

</details>

- **Switch:** `switch (t) { case v: … default: … }` — int-constant cases,
  fall-through allowed.

<details>
<summary>Example: switch with fall-through</summary>

```flint
// build: flintc s5_switch.flint -o s5_switch && ./s5_switch
int main() {
    int m = 2;
    switch (m) {
        case 1:
            print("one");
            break;
        case 2:
            print("two");
            break;
        default:
            print("other");
    }
    print("\n");
    int n = 5;
    switch (n) {
        case 5:
            print("five");
            print("-");
        case 6:
            print("six");
            break;
        default:
            print("other");
    }
    print("\n");
    return 0;
}
```

Expected output (the second switch falls through `case 5` into `case 6`):

```
two
five-six
```

</details>

- **Labeled loops:** `label: for/while …`; `break label` exits that loop from
  anywhere inside it; `continue label` jumps to the next iteration of that
  (outer) loop (see `examples/labels.flint`).

<details>
<summary>Example: labeled loop with `break label`</summary>

```flint
// build: flintc s5_labels.flint -o s5_labels && ./s5_labels
int main() {
    int i = 0;
    int j = 0;
    outer:
    for (i = 0; i < 3; i = i + 1) {
        for (j = 0; j < 3; j = j + 1) {
            if (i * 3 + j == 7) {
                break outer;
            }
        }
    }
    print(i * 3 + j);
    print("\n");
    return 0;
}
```

Expected output (stops at i=2, j=1):

```
7
```

</details>

- `break` / `continue` (plain or labeled).

<details>
<summary>Example: `break` / `continue`</summary>

```flint
// build: flintc s5_breakcont.flint -o s5_breakcont && ./s5_breakcont
int main() {
    int total = 0;
    for (int i = 0; i < 10; i = i + 1) {
        if (i < 2) {
            continue;
        }
        if (i > 4) {
            break;
        }
        total = total + i;
    }
    print(total);
    print("\n");
    return 0;
}
```

Expected output (only i=2,3,4 are added):

```
9
```

</details>

- **`defer expr;`** — `expr` is evaluated when the enclosing function exits
  (Go-style: last deferred runs first, runs on every return path).

<details>
<summary>Example: `defer` (LIFO on function exit)</summary>

```flint
// build: flintc s5_defer.flint -o s5_defer && ./s5_defer
void log(int n) {
    println(n);
}
int main() {
    defer log(1);
    defer log(2);
    log(3);
    return 0;
}
```

Expected output:

```
3
2
1
```

</details>

---

## 6. Functions

- **Free functions:** `int name(int a, string s) { … }` (an optional return
  type; `void` or omitted).

<details>
<summary>Example: free functions (with and without return type)</summary>

```flint
// build: flintc s6_freefn.flint -o s6_freefn && ./s6_freefn
int doubleIt(int x) {
    return x * 2;
}
void greet(string who) {
    print("hi " + who);
}
int main() {
    print(doubleIt(21));
    print("\n");
    greet("flint");
    print("\n");
    return 0;
}
```

Expected output:

```
42
hi flint
```

</details>

- **Generic free functions:** `T identity<T>(T x) { … }`.

<details>
<summary>Example: generic free function</summary>

```flint
// build: flintc s6_genericfn.flint -o s6_genericfn && ./s6_genericfn
T identity<T>(T x) {
    return x;
}
int main() {
    print(identity(7));
    print("\n");
    print(identity("hi"));
    print("\n");
    return 0;
}
```

Expected output:

```
7
hi
```

</details>

- **Default parameter values:** trailing parameters may carry defaults
  (`void f(int a, int b = 0)`; called as `f(1)` or `f(1, 2)`). Defaults must
  be **constants** (literals, enum variants, or `Class.staticField` references —
  no calls or expressions) and are **not allowed on constructors or lambda
  parameters**. Once a parameter carries a default, all following ones must too.

<details>
<summary>Example: default parameters (literal and static-field)</summary>

```flint
// build: flintc s6_defaultparam.flint -o s6_defaultparam && ./s6_defaultparam
int add(int a, int b = 10) {
    return a + b;
}
class Consts {
    static int fallback;
}
int fromStatic(int x = Consts.fallback) {
    return x;
}
int main() {
    print(add(1));
    print("\n");
    print(add(1, 2));
    print("\n");
    Consts.fallback = 42;
    print(fromStatic());
    print("\n");
    return 0;
}
```

Expected output:

```
11
3
42
```

</details>

- **Static methods** (`static`), **instance methods**, **constructors**
  (`Name(…)` or `void Name(…)`).

<details>
<summary>Example: static method, instance method, constructor</summary>

```flint
// build: flintc s6_methods.flint -o s6_methods && ./s6_methods
class Counter {
    int n;
    static int made;
    public Counter() {
        this.n = 0;
        Counter.made = Counter.made + 1;
    }
    public int inc() {
        this.n = this.n + 1;
        return this.n;
    }
    public static int total() {
        return Counter.made;
    }
}
int main() {
    Counter c = new Counter();
    print(c.inc());
    print("\n");
    print(c.inc());
    print("\n");
    print(Counter.total());
    print("\n");
    return 0;
}
```

Expected output:

```
1
2
1
```

</details>

- **`async` functions:** a call runs the body on a worker thread and evaluates
  to a `std.Task` (see Concurrency).

<details>
<summary>Example: `async` function → `std.Task`</summary>

```flint
// build: flintc src/stdlib/*.flint s6_async.flint -o s6_async && ./s6_async
import std.Task;
async int square(int x) {
    return x * x;
}
int main() {
    Task t = square(9);
    print(t.join());
    print("\n");
    return 0;
}
```

Expected output:

```
81
```

</details>

- **Multi-value returns:** a function may return a tuple type
  `(T1, T2, …)`; the caller destructures it (see Tuples).

<details>
<summary>Example: multi-value return</summary>

```flint
// build: flintc s6_multival.flint -o s6_multival && ./s6_multival
(int, int) pair(int n) {
    return (n, n * 2);
}
int main() {
    (int a, int b) = pair(21);
    print(a + b);
    print("\n");
    return 0;
}
```

Expected output:

```
63
```

</details>

- **Qualified generic free-function calls** with explicit type args:
  `std.map<T, U>(xs, f)`.

<details>
<summary>Example: `std.map<T, U>` (qualified generic call)</summary>

```flint
// build: flintc src/stdlib/*.flint s6_qualifiedgeneric.flint -o s6_qualifiedgeneric && ./s6_qualifiedgeneric
import std.List;
int main() {
    List<int> xs = new List<int>();
    xs.add(1);
    xs.add(2);
    xs.add(3);
    List<int> ys = std.map<int, int>(xs, (int x) -> int { return x * x; });
    int sum = 0;
    for (int i = 0; i < ys.size(); i = i + 1) {
        sum = sum + ys.get(i);
    }
    print(sum);
    print("\n");
    return 0;
}
```

Expected output (1² + 2² + 3²):

```
14
```

</details>

---

## 7. Classes & OOP

- **Fields:** `int n;`, `string s;`, with optional `public` / `private`
  visibility, optional `static`, and optional **accessor** `get` / `set` /
  `getset` (properties).

<details>
<summary>Example: fields (visibility, static, accessors)</summary>

```flint
// build: flintc s7_fields.flint -o s7_fields && ./s7_fields
class User {
    private get int _age;
    private getset string _name;
    public int pub;
    static int count;
    public User(int age, string name) {
        this._age = age;
        this._name = name;
        this.pub = age + 1;
        User.count = User.count + 1;
    }
}
int main() {
    User u = new User(30, "bob");
    print(u.getAge());
    print("\n");
    print(u.getName());
    print("\n");
    u.setName("alice");
    print(u.getName());
    print("\n");
    print(u.pub);
    print("\n");
    print(User.count);
    print("\n");
    return 0;
}
```

Expected output:

```
30
bob
alice
31
1
```

</details>

- **Constructors:** `public Name(int x) { … }` (or `void Name(…)`).

<details>
<summary>Example: constructors (overloaded)</summary>

```flint
// build: flintc s7_ctor.flint -o s7_ctor && ./s7_ctor
class Box {
    int v;
    public Box() {
        this.v = 1;
    }
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box();
    print(a.v);
    print("\n");
    Box b = new Box(5);
    print(b.v);
    print("\n");
    return 0;
}
```

Expected output:

```
1
5
```

</details>

- **Methods:** instance and `static`; optional `public` / `private`; optional
  `async`; optional `abstract`.

<details>
<summary>Example: instance, static and `async` methods</summary>

Note: an `async` method cannot live on a class with a vtable (an
`extends`/`abstract` hierarchy) — the compiler reports
`async method … cannot be virtual (the class has a vtable)`.

```flint
// build: flintc src/stdlib/*.flint s7_methods.flint -o s7_methods && ./s7_methods
import std.Task;
class Square {
    int side;
    public Square(int s) {
        this.side = s;
    }
    public int area() {
        return this.side * this.side;
    }
    async int areaAsync() {
        return this.side * this.side;
    }
    static int tag() {
        return 1;
    }
}
int main() {
    Square s = new Square(3);
    print(s.area());
    print("\n");
    Square q = new Square(4);
    Task t = q.areaAsync();
    print(t.join());
    print("\n");
    print(Square.tag());
    print("\n");
    return 0;
}
```

Expected output:

```
9
16
1
```

</details>

- **Inheritance:** `class Child extends Parent { … }` — `super` as a receiver
  (`super.field`, `super.method(…)`) and `super(args)` to invoke the parent
  constructor.

<details>
<summary>Example: inheritance (`super`, override, virtual dispatch)</summary>

```flint
// build: flintc s7_inherit.flint -o s7_inherit && ./s7_inherit
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
    public int speak() {
        return 1;
    }
    public int bark() {
        return super.speak() + this.barks;
    }
}
int main() {
    Animal a = new Dog(5, 3);
    print(a.speak());
    print("\n");
    print(a.id);
    print("\n");
    Dog d = (Dog) a;
    print(d.bark());
    print("\n");
    return 0;
}
```

Expected output:

```
1
5
3
```

</details>

- **Interfaces:** `interface Name { void m(…); }` — a set of abstract method
  signatures (v1: no fields, no generic interfaces, no interface inheritance).
  A class adopts them with `implements A, B, …`.

<details>
<summary>Example: interfaces (`implements`, virtual dispatch)</summary>

```flint
// build: flintc s7_interface.flint -o s7_interface && ./s7_interface
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
class Human implements Speaker {
    int vol;
    public Human(int v) {
        this.vol = v;
    }
    public int speak() {
        return this.vol + 100;
    }
}
int main() {
    Speaker s = new Robot(3);
    print(s.speak());
    print("\n");
    Speaker h = new Human(5);
    print(h.speak());
    print("\n");
    return 0;
}
```

Expected output:

```
30
105
```

</details>

- **Abstract classes:** `abstract class Name { … }` — cannot be instantiated;
  may declare abstract methods (a concrete class must override them).
  `abstract` methods are declared with `;` and no body.

<details>
<summary>Example: abstract class with an abstract method</summary>

```flint
// build: flintc s7_abstract.flint -o s7_abstract && ./s7_abstract
abstract class Animal {
    abstract int legs();
    int describe() {
        return this.legs() * 10;
    }
}
class Dog extends Animal {
    public Dog() {
    }
    public int legs() {
        return 4;
    }
}
int main() {
    Animal a = new Dog();
    print(a.legs());
    print("\n");
    print(a.describe());
    print("\n");
    return 0;
}
```

Expected output:

```
4
40
```

</details>

- **`instanceof`:** `e instanceof Type` — true when `e`'s dynamic type is
  `Type` or a subtype.

<details>
<summary>Example: `instanceof`</summary>

```flint
// build: flintc s7_instanceof.flint -o s7_instanceof && ./s7_instanceof
interface Speaker {
    int speak();
}
class Robot implements Speaker {
    public int speak() {
        return 1;
    }
}
class Human implements Speaker {
    public int speak() {
        return 2;
    }
}
int main() {
    Speaker s = new Robot();
    print(s instanceof Robot);
    print("\n");
    print(s instanceof Human);
    print("\n");
    print(s instanceof Speaker);
    print("\n");
    return 0;
}
```

Expected output:

```
1
0
1
```

</details>

- **Casts:** `(Type) e` — up/downcast a class value (upcast is a no-op;
  downcast is unchecked in v1). `(byte)` / `(short)` / `(char)` reinterpret a
  buffer in place (a byte/short view of the same memory); `(int)` is a slot-view
  reinterpret when the source is a buffer.

<details>
<summary>Example: casts (class up/down, buffer view)</summary>

```flint
// build: flintc s7_cast.flint -o s7_cast && ./s7_cast
class Animal {
    int id;
    public Animal(int i) {
        this.id = i;
    }
}
class Dog extends Animal {
    int barks;
    public Dog(int i, int b) {
        super(i);
        this.barks = b;
    }
}
int main() {
    Dog d = new Dog(1, 2);
    Animal a = d;
    print(a.id);
    print("\n");
    Dog d2 = (Dog) a;
    print(d2.barks);
    print("\n");
    byte b[4];
    b[0] = 1;
    b[1] = 0;
    short s[] = (short) b;
    print(s[0]);
    print("\n");
    return 0;
}
```

Expected output (the byte pair `1 0` read as one little-endian short is 1):

```
1
2
1
```

</details>

- **`new`:** `new ClassName(args)` and **struct literals**
  `new ClassName { field: value, … }` (with optional explicit type args,
  e.g. `new Vessel<int>(5)`).

<details>
<summary>Example: `new` (constructor call and positional field init)</summary>

Note: the compiler requires parentheses after the class name — the
brace form `new Point { x: 5, y: 6 }` is rejected
(`expected '(' after class name, found LBrace`). A `new` without a matching
constructor fills the fields positionally.

```flint
// build: flintc s7_new.flint -o s7_new && ./s7_new
class Point {
    int x;
    int y;
    public Point(int a, int b) {
        this.x = a;
        this.y = b;
    }
}
class Pair {
    int a;
    int b;
}
int main() {
    Point p = new Point(3, 4);
    print(p.x + p.y);
    print("\n");
    Pair q = new Pair(5, 6);
    print(q.a + q.b);
    print("\n");
    return 0;
}
```

Expected output:

```
7
11
```

</details>

- **`this`** — the method receiver (never moves).

<details>
<summary>Example: `this`</summary>

```flint
// build: flintc s7_this.flint -o s7_this && ./s7_this
class Acc {
    int n;
    public Acc(int x) {
        this.n = x;
    }
    public int add(int k) {
        this.n = this.n + k;
        return this.n;
    }
}
int main() {
    Acc a = new Acc(10);
    print(a.add(5));
    print("\n");
    print(a.add(7));
    print("\n");
    return 0;
}
```

Expected output:

```
15
22
```

</details>

- **Class constants:** `const NAME = <value>;` as a class member, referenced as
  `Class.NAME`.

<details>
<summary>Example: class constants</summary>

```flint
// build: flintc s7_const.flint -o s7_const && ./s7_const
class Limits {
    const MAX = 100;
    const NAME = "lim";
    const ON = true;
}
int main() {
    print(Limits.MAX);
    print("\n");
    print(Limits.NAME);
    print("\n");
    print(Limits.ON);
    print("\n");
    return 0;
}
```

Expected output:

```
100
lim
1
```

</details>

- **Enums:** `enum Name { A, B = 5, C }` — named integer constants; values are
  sequential when not explicitly set.

<details>
<summary>Example: enums (explicit and sequential values)</summary>

```flint
// build: flintc s7_enum.flint -o s7_enum && ./s7_enum
enum Level {
    LOW,
    MID = 5,
    HIGH,
}
int main() {
    print(Level.LOW);
    print("\n");
    print(Level.MID);
    print("\n");
    print(Level.HIGH);
    print("\n");
    return 0;
}
```

Expected output:

```
0
5
6
```

</details>

---

## 8. Generics

- **Generic classes:** `class Vessel<T> { … }` — instantiated as
  `Vessel<int>`, `Vessel<Pair<int, int>>`, …

<details>
<summary>Example: generic class</summary>

```flint
// build: flintc s8_genericclass.flint -o s8_genericclass && ./s8_genericclass
class Vessel<T> {
    T v;
    public Vessel(T x) {
        this.v = x;
    }
    public T get() {
        return this.v;
    }
}
int main() {
    Vessel<int> a = new Vessel<int>(5);
    print(a.get());
    print("\n");
    Vessel<string> b = new Vessel<string>("hi");
    print(b.get());
    print("\n");
    return 0;
}
```

Expected output:

```
5
hi
```

</details>

- **Generic functions:** `T identity<T>(T x) { … }`.

<details>
<summary>Example: generic function</summary>

```flint
// build: flintc s8_genericfn.flint -o s8_genericfn && ./s8_genericfn
T first<T>(T a, T b) {
    return a;
}
int main() {
    print(first(1, 2));
    print("\n");
    print(first("a", "b"));
    print("\n");
    return 0;
}
```

Expected output:

```
1
a
```

</details>

- **Type inference:** type arguments are inferred from argument types when
  omitted (a type parameter no parameter uses falls back to `int`).

<details>
<summary>Example: type inference (generic function call)</summary>

Note: inference applies to generic *function* calls; `new` on a generic class
requires explicit type args (`new Box(9)` is rejected with
`class 'Box' expects 1 type argument(s), got 0`).

```flint
// build: flintc s8_infer.flint -o s8_infer && ./s8_infer
T identity<T>(T x) {
    return x;
}
int main() {
    print(identity(7));
    print("\n");
    print(identity("hi"));
    print("\n");
    return 0;
}
```

Expected output:

```
7
hi
```

</details>

- **Explicit type arguments:** `identity<int>(5)`, `new Vessel<int>(5)`,
  `std.map<T, U>(xs, f)`.

<details>
<summary>Example: explicit type arguments</summary>

```flint
// build: flintc s8_explicit.flint -o s8_explicit && ./s8_explicit
T identity<T>(T x) {
    return x;
}
class Box<T> {
    T v;
    public Box(T x) {
        this.v = x;
    }
    public T get() {
        return this.v;
    }
}
int main() {
    print(identity<int>(7));
    print("\n");
    Box<string> b = new Box<string>("hi");
    print(b.get());
    print("\n");
    return 0;
}
```

Expected output:

```
7
hi
```

</details>

- **Monomorphization:** the middle pass expands every generic type/function that
  is actually used (via a worklist, so nested instantiations like
  `Vessel<Pair<int, string>>` are handled) and rewrites the AST so the backend
  only ever sees concrete types and mangled names.

<details>
<summary>Example: nested instantiation (`Vessel<Vessel<int>>`)</summary>

```flint
// build: flintc s8_mono.flint -o s8_mono && ./s8_mono
class Vessel<T> {
    T v;
    public Vessel(T x) {
        this.v = x;
    }
    public T get() {
        return this.v;
    }
}
int main() {
    Vessel<Vessel<int>> nested = new Vessel<Vessel<int>>(new Vessel<int>(5));
    print(nested.get().get());
    print("\n");
    return 0;
}
```

Expected output:

```
5
```

</details>

---

## 9. Lambdas & closures

- **Java-style lambda syntax:**
  - `int x -> boolean { … }` (single param, parenless)
  - `(int a, int b) -> int { … }` (param list)
  - `() -> void { … }` (no params)

<details>
<summary>Example: all three lambda syntax forms</summary>

Note: a no-param closure is invoked as `sys.fnCall2(block, 0)` — the call
takes a fixed argument slot.

```flint
// build: flintc s9_syntax.flint -o s9_syntax && ./s9_syntax
int main() {
    int doubleIt = (int x) -> int { return x * 2; };
    print(sys.fnCall2(doubleIt, 21));
    print("\n");
    int add = (int a, int b) -> int { return a + b; };
    print(sys.fnCall3(add, 3, 4));
    print("\n");
    int hello = () -> void { print("hi"); };
    sys.fnCall2(hello, 0);
    print("\n");
    return 0;
}
```

Expected output:

```
42
7
hi
```

</details>

- **Lifting:** the middle pass lifts a lambda into a generated top-level
  function (whose first parameter is the `*int` capture block) and replaces it
  with a **Closure** — a pointer to a fresh heap block
  `[fn_ptr, cap0, cap1, …]`. Captures are evaluated (by value) when the block
  is created. Calling convention: `fn(ctx, arg1, arg2)`.

<details>
<summary>Example: capture is by value at block creation</summary>

```flint
// build: flintc s9_lift.flint -o s9_lift && ./s9_lift
int main() {
    int base = 100;
    int addBase = (int x) -> int { return x + base; };
    print(sys.fnCall2(addBase, 5));
    print("\n");
    base = 200;
    print(sys.fnCall2(addBase, 5));
    print("\n");
    return 0;
}
```

Expected output (both calls see the captured value 100, not the later 200):

```
105
105
```

</details>

- **Calling a closure:** `sys.fnCall2(block, a)` (one arg) and
  `sys.fnCall3(block, a, b)` (two args) — `block` is `[fn_ptr][cap0][cap1]…`.

<details>
<summary>Example: `sys.fnCall2` / `sys.fnCall3`</summary>

```flint
// build: flintc s9_fnCall.flint -o s9_fnCall && ./s9_fnCall
int main() {
    int one = (int a) -> int { return a * 10; };
    print(sys.fnCall2(one, 4));
    print("\n");
    int two = (int a, int b) -> int { return a + b; };
    print(sys.fnCall3(two, 20, 22));
    print("\n");
    return 0;
}
```

Expected output:

```
40
42
```

</details>

- **Higher-order stdlib combinators** (pass a lambda value as a `*int`):
  - `std.map<T, U>(xs, f)` — fresh `List<U>` with `f` applied to each element.
  - `std.reduce<T, U>(xs, f, init)` — fold: `f(acc, elem) -> acc`.
  - `std.forEach<T>(xs, f)` — call `f(elem)` for every element (no result).
  - `Arrays.filter(a, pred)` — the elements of a raw `int[]` for which the
    predicate closure is true, as a fresh `int[]`.
  - `List`/`Queue`/`CopyOnWriteList` `.filter(*int pred)` and `.forEach(*int f)`.

<details>
<summary>Example: `std.map`, `std.reduce`, `std.forEach`, `Arrays.filter`</summary>

```flint
// build: flintc src/stdlib/*.flint s9_combinators.flint -o s9_combinators && ./s9_combinators
import std.List;
import std.Arrays;
int main() {
    List<int> xs = new List<int>();
    xs.add(1);
    xs.add(2);
    xs.add(3);
    List<int> doubled = std.map<int, int>(xs, (int x) -> int { return x * 2; });
    int dsum = 0;
    for (int i = 0; i < doubled.size(); i = i + 1) {
        dsum = dsum + doubled.get(i);
    }
    print(dsum);
    print("\n");
    int total = std.reduce<int, int>(xs, (int acc, int x) -> int { return acc + x; }, 0);
    print(total);
    print("\n");
    std.forEach<int>(xs, (int x) -> void { print(x); print(" "); });
    print("\n");
    int raw[] = [1, 2, 3, 4, 5];
    int ev[] = Arrays.filter(raw, (int x) -> int { return x % 2 == 0; });
    int esum = 0;
    for (int i = 0; i < len(ev); i = i + 1) {
        esum = esum + ev[i];
    }
    print(esum);
    print("\n");
    return 0;
}
```

Expected output (the 3rd line is `1 2 3 ` with a trailing space):

```
12
6
1 2 3 
6
```

</details>
---

## 10. Memory model

Ownership is checked at compile time (`src/middle/ownership.rs`, run after
monomorphization) — like Rust Ch 4.1: each value has a single owner; when the
owner goes out of scope the value is dropped.

- **Moves:** `T x = y`, `x = y`, `return y`, and `obj.field = y` *move* a class
  object, a `*T` buffer, a `string`, or an array — `y` becomes invalid and
  using it afterwards is a compile error. `*int q = p` moves `p`;
  `string b = a` moves `a`.

<details>
<summary>Example: a move invalidates the source</summary>

```flint
// build: flintc s10_move.flint -o s10_move && ./s10_move
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(1);
    Box b = a;
    print(b.v);
    print("\n");
    return 0;
}
```

Expected output:

```
1
```

Using `a` after the move is a compile error:

```flint
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(1);
    Box b = a;
    print(a.v);
    return 0;
}
```

```
error[10:11]: use of moved value `a` (each value has a single owner; `a` was moved or freed here — clone with `a.move()` (shared) or `str.copy(a)` (independent string); see https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
  |     print(a.v);
  |           ^
```

</details>

- **Borrows:** calls borrow like `&T` — `x.foo()`, `f(x)`, `str.*`, and `sys.*`
  leave their arguments valid; `&x` borrows; `this` never moves. `int` / `bool`
  / enum are `Copy`.

<details>
<summary>Example: calls borrow — the argument stays valid</summary>

```flint
// build: flintc s10_borrow.flint -o s10_borrow && ./s10_borrow
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
    public int peek() {
        return this.v;
    }
}
int doubleIt(int x) {
    return x * 2;
}
int main() {
    Box a = new Box(5);
    print(a.peek());
    print("\n");
    print(doubleIt(a.v));
    print("\n");
    print(a.peek());
    print("\n");
    return 0;
}
```

Expected output (`a` is still usable after the calls):

```
5
10
5
```

</details>

- **Owned values free themselves:** dropping means a real `munmap` at scope end
  (or reassignment), verified by the `frees` test — no `free` call exists. There
  is no `alloc` either: buffers come only from typed initialization
  (`int buf[n]`, `byte buf[n]`, `short buf[n]`, `string s[n]`), array/string
  literals, and calls that return fresh values. Raw `*T` buffers have no
  `move()` (they follow explicit free discipline).

<details>
<summary>Example: drop at scope end (no `free` call anywhere)</summary>

```flint
// build: flintc s10_free.flint -o s10_free && ./s10_free
class Tracked {
    int id;
    static int alive;
    public Tracked(int i) {
        this.id = i;
        Tracked.alive = Tracked.alive + 1;
    }
    public void destroy() {
        Tracked.alive = Tracked.alive - 1;
        print("freed " + str.itoa(this.id));
        print("\n");
    }
}
int use(Tracked t) {
    return 0;
}
int scope() {
    Tracked t = new Tracked(1);
    use(t);
    return 0;
}
int main() {
    print("start");
    print("\n");
    scope();
    print("alive=" + str.itoa(Tracked.alive));
    print("\n");
    return 0;
}
```

Expected output (the object frees itself when `scope()` returns — there is no
`free` call in the source):

```
start
freed 1
alive=0
```

</details>

- **Never store a view in an owned slot:** `buf + off`, `&x`, `*p`, and `@f`
  have no header to retain, so passing one where an owned value is required
  (argument, field, return, binding) is a compile error — materialize it first
  (e.g. `str.substring`).

<details>
<summary>Example: view in an owned slot → compile error</summary>

```flint
// build: flintc s10_view_err.flint -o s10_view_err   (fails to compile)
int main() {
    string s = "hello";
    string v = s + 1;
    return 0;
}
```

Expected output (compiler error):

```
error[4:16]: cannot store pointer arithmetic in an owned slot; it has no header to retain (materialize it first, e.g. with str.substring)
  |     string v = s + 1;
  |                ^
```

</details>

- **Clone explicitly with `x.move()`:** available on any object, `string`, or
  array value — it borrows the receiver and returns a fresh owner of the same
  type (on objects `mem.retainVal`; on strings/arrays a plain shared alias).
  Use it for shared ownership: `Pair c = h.move();` keeps `h` valid, as does
  `string b = a.move();`. Use `str.copy(s)` for an independent string. A
  user-defined `move` method wins over this builtin clone; `move()` with args,
  on a class name, or on any other type is a compile error.

<details>
<summary>Example: `x.move()` keeps the receiver valid</summary>

```flint
// build: flintc s10_moveclone.flint -o s10_moveclone && ./s10_moveclone
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(1);
    Box b = a.move();
    print(a.v);
    print("\n");
    print(b.v);
    print("\n");
    string s = "hi";
    string t = s.move();
    print(s);
    print("\n");
    print(t);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
hi
hi
```

</details>

- **`destroy()` hook:** a class may define `destroy()`, called automatically
  when the owner goes out of scope (the collections use it to release element
  references).

<details>
<summary>Example: `destroy()` runs when the owner's scope ends</summary>

```flint
// build: flintc s10_destroy.flint -o s10_destroy && ./s10_destroy
class Tracked {
    int id;
    public Tracked(int i) {
        this.id = i;
    }
    public void destroy() {
        print("freed " + str.itoa(this.id));
        print("\n");
    }
}
int use(Tracked t) {
    return 0;
}
int make() {
    Tracked t = new Tracked(7);
    use(t);
    return 0;
}
int main() {
    make();
    print("main");
    print("\n");
    return 0;
}
```

Expected output:

```
freed 7
main
```

</details>

- **Escape analysis** (`src/backend/escape.rs`): decides per class-typed local
  whether its object is stack-allocated (non-escaping) or heap-allocated with a
  refcount header `[refcount][field0][field1]…`. An object escapes (forces the
  heap) when it is passed as an argument, is the base of a method call, is
  returned, is stored in a field, is address-taken (`&x`), or is the target of
  an assignment. Copies retain (`flint_retain`); drops release (`flint_release`).
  Cycles leak in v1 (no weak references).

<details>
<summary>Example: non-escaping vs escaping object</summary>

```flint
// build: flintc s10_escape.flint -o s10_escape && ./s10_escape
class Local {
    int v;
    public Local(int x) {
        this.v = x;
    }
    public int get() {
        return this.v;
    }
}
int use(Local l) {
    return l.get();
}
int main() {
    Local a = new Local(1);
    print(a.get());
    print("\n");
    Local b = new Local(2);
    print(use(b));
    print("\n");
    return 0;
}
```

Expected output (`a` never escapes — stack-allocated; `b` is passed to
`use` — heap-allocated with a refcount header):

```
1
2
```

</details>

- **Refcounting primitives:** `mem.retain(o)`, `mem.release(o)`,
  `mem.retainVal(o)` (null-guarded, returns `o`), `mem.retainStr(v)` /
  `mem.releaseStr(v)` (strings held as raw int slots — the collections' unsafe
  core).

<details>
<summary>Example: `mem.retain` / `mem.release` / `mem.retainVal`</summary>

```flint
// build: flintc s10_refcount.flint -o s10_refcount && ./s10_refcount
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(5);
    mem.retain(a);
    mem.release(a);
    print(a.v);
    print("\n");
    int r = mem.retainVal(0);
    print(r == 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output (`retainVal` is null-guarded and returns its argument):

```
5
1
```

</details>

- **Raw pointers:** `*T` — `null`, `&x` (address-of), `*p` (deref), `@f`
  (function address, via `sys.fnAddr` / `flint_fn_addr`), and calling through a
  pointer (`flint_call_ptr`).

<details>
<summary>Example: `&x`, `*p`, `@f`</summary>

Note: `flint_call_ptr` is a runtime-only entry point — there is no Flint
source syntax for calling through a function pointer, and
`sys.fnCall2(@f, x)` segfaults (it expects a closure block, not a bare
address).

```flint
// build: flintc s10_rawptr.flint -o s10_rawptr && ./s10_rawptr
int doubleIt(int x) {
    return x * 2;
}
int main() {
    *int fn = @doubleIt;
    print(fn != null ? 1 : 0);
    print("\n");
    int x = 21;
    *int p = &x;
    print(*p);
    print("\n");
    *p = 99;
    print(x);
    print("\n");
    return 0;
}
```

Expected output:

```
1
21
99
```

</details>

- **Sized load/store & direct syscalls** for raw memory access.

<details>
<summary>Example: direct syscalls (`sys.open`/`write`/`close`, `sys.mmap`)</summary>

```flint
// build: flintc s10_syscall.flint -o s10_syscall && ./s10_syscall
int main() {
    int fd = sys.open("/dev/null", 2, 0);
    print(fd >= 0 ? 1 : 0);
    print("\n");
    string msg = "hi";
    sys.write(fd, msg, strlen(msg));
    sys.close(fd);
    int p = sys.mmap(0, 64, 3, 0x22, -1, 0);
    print(p >= 0 ? 1 : 0);
    print("\n");
    sys.munmap(p, 64);
    print("done");
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
done
```

</details>

---

## 11. Exception handling

- **`throw expr;`** — stores `expr` in the global exception slot and returns.

<details>
<summary>Example: `throw`</summary>

```flint
// build: flintc s11_throw.flint -o s11_throw && ./s11_throw
int main() {
    try {
        throw 7;
    } catch (int e) {
        print(e);
        print("\n");
    }
    return 0;
}
```

Expected output:

```
7
```

</details>

- **`try { … } catch (T var) { … }`** / **`catch (A | B var) { … }`** —
  multi-type catch; the catch variable is typed by the least upper bound (a
  common parent class or interface).

<details>
<summary>Example: multi-type catch (`A | B`)</summary>

```flint
// build: flintc s11_multicatch.flint -o s11_multicatch && ./s11_multicatch
class AppError {
    int code;
    public AppError(int c) {
        this.code = c;
    }
}
class NetError extends AppError {
    public NetError(int c) {
        super(c);
    }
}
class TimeoutError extends AppError {
    public TimeoutError(int c) {
        super(c);
    }
}
int main() {
    try {
        throw new NetError(1);
    } catch (NetError | TimeoutError e) {
        print(e.code);
        print("\n");
    }
    return 0;
}
```

Expected output (the catch variable is typed `AppError`, the common parent):

```
1
```

</details>

- **`finally { … }`** — runs on normal completion, on a caught exception, and on
  a `return` from the try/catch blocks.

<details>
<summary>Example: `finally` after a caught exception</summary>

```flint
// build: flintc s11_finally.flint -o s11_finally && ./s11_finally
int main() {
    try {
        print("try");
        print("\n");
        throw 1;
    } catch (int e) {
        print("catch");
        print("\n");
    } finally {
        print("finally");
        print("\n");
    }
    return 0;
}
```

Expected output:

```
try
catch
finally
```

</details>

---

## 12. Pattern matching (`match`)

`match <scrutinee> { <pat> => <expr>, … }` — an expression (desugared by the
monomorphizer into a nested `Cond` chain). Supported scrutinee types: `int`,
`char` (a code point), `enum`, and `string`.

- **Patterns:** an integer (or char) literal, a string literal, an enum variant
  (`Enum.Variant`), and the wildcard `_` (matches anything; the fallback).

<details>
<summary>Example: all pattern kinds (int, char, enum, string, wildcard)</summary>

```flint
// build: flintc s12_match.flint -o s12_match && ./s12_match
enum Color {
    RED,
    GREEN,
}
int grade(int n) {
    return match n {
        1 => 100,
        2 => 200,
        _ => 0,
    };
}
int letter(char c) {
    return match c {
        'a' => 1,
        _ => 9,
    };
}
int col(Color c) {
    return match c {
        Color.RED => 1,
        Color.GREEN => 2,
        _ => 0,
    };
}
int tag(string s) {
    return match s {
        "hello" => 1,
        _ => 0,
    };
}
int main() {
    print(grade(1));
    print("\n");
    print(grade(9));
    print("\n");
    print(letter('a'));
    print("\n");
    print(col(Color.GREEN));
    print("\n");
    print(tag("hello"));
    print("\n");
    return 0;
}
```

Expected output:

```
100
0
1
2
1
```

</details>

---

## 13. Tuples (multi-value returns)

- **Tuple type:** `(T1, T2, …)` — a multi-value return type (≥ 2 members).

<details>
<summary>Example: tuple return type</summary>

```flint
// build: flintc s13_tupletype.flint -o s13_tupletype && ./s13_tupletype
(int, string) pair(int n) {
    return (n * 2, "n=" + str.itoa(n));
}
int main() {
    (int a, string s) = pair(21);
    print(a);
    print("\n");
    print(s);
    print("\n");
    return 0;
}
```

Expected output:

```
42
n=21
```

</details>

- **Tuple literal:** `(e1, e2, …)` — meaningful as a `return` value or the RHS
  of a tuple destructure.

<details>
<summary>Example: tuple literal (destructured directly)</summary>

```flint
// build: flintc s13_tuplelit.flint -o s13_tuplelit && ./s13_tuplelit
int main() {
    (int a, string s) = (10, "lit");
    print(a);
    print("\n");
    print(s);
    print("\n");
    return 0;
}
```

Expected output:

```
10
lit
```

</details>

- **Destructure into fresh locals:** `(T1 a, T2 b) = value;`

<details>
<summary>Example: destructure into fresh locals</summary>

```flint
// build: flintc s13_destructfresh.flint -o s13_destructfresh && ./s13_destructfresh
(int, int) two(int n) {
    return (n, n + 1);
}
int main() {
    (int a, int b) = two(5);
    print(a + b);
    print("\n");
    return 0;
}
```

Expected output:

```
11
```

</details>

- **Destructure with inferred types:** `var (a, b) = value;`

<details>
<summary>Example: destructure with `var`</summary>

```flint
// build: flintc s13_destructvar.flint -o s13_destructvar && ./s13_destructvar
(int, string) two(int n) {
    return (n, "v" + str.itoa(n));
}
int main() {
    var (a, b) = two(7);
    print(a);
    print("\n");
    print(b);
    print("\n");
    return 0;
}
```

Expected output:

```
7
v7
```

</details>

- **Destructure into existing locals:** `(a, b) = value;`

<details>
<summary>Example: destructure into existing locals</summary>

```flint
// build: flintc s13_destructexist.flint -o s13_destructexist && ./s13_destructexist
(int, string) two(int n) {
    return (n, "x" + str.itoa(n));
}
int main() {
    int x = 0;
    string y = "old";
    (x, y) = two(3);
    print(x);
    print("\n");
    print(y);
    print("\n");
    return 0;
}
```

Expected output:

```
3
x3
```

</details>

---

## 14. Concurrency

- **`async` functions/methods:** a call runs the body on its own worker thread
  and evaluates to a `std.Task`.

<details>
<summary>Example: `async` function</summary>

```flint
// build: flintc src/stdlib/*.flint s14_async.flint -o s14_async && ./s14_async
import std.Task;
async int square(int x) {
    return x * x;
}
int main() {
    Task t = square(9);
    print(t.join());
    print("\n");
    return 0;
}
```

Expected output:

```
81
```

</details>

- **`Task`** (`import std.Task`): `join()` blocks until the worker finishes and
  returns its value (0 for a void function).

<details>
<summary>Example: `Task.join()` (value and void)</summary>

```flint
// build: flintc src/stdlib/*.flint s14_task.flint -o s14_task && ./s14_task
import std.Task;
async int add(int a, int b) {
    return a + b;
}
async void nothing() {
    return;
}
int main() {
    Task t = add(20, 22);
    print(t.join());
    print("\n");
    Task v = nothing();
    print(v.join());
    print("\n");
    return 0;
}
```

Expected output (a void function yields 0):

```
42
0
```

</details>

- **`await e`:** blocks until the task finishes and evaluates to the value it
  produced. `e` is a call to an `async` function/method (free, instance, or
  static) or an expression of type `std.Task`. A void function yields 0;
  awaiting a bare `Task` yields an int. An async function may await another.
  **At most 32 tasks** may be in flight at once; spawning a 33rd waits for a
  slot to free.

<details>
<summary>Example: `await` (direct call and nested await)</summary>

```flint
// build: flintc src/stdlib/*.flint s14_await.flint -o s14_await && ./s14_await
import std.Task;
async int doubleIt(int x) {
    return x * 2;
}
async int innerPlus(int x) {
    int d = await doubleIt(x);
    return d + 1;
}
int main() {
    print(await doubleIt(21));
    print("\n");
    print(await innerPlus(10));
    print("\n");
    return 0;
}
```

Expected output:

```
42
21
```

</details>

- **`Thread`** (`import std.Thread`): `nCpu()`, `spawn(fn, arg)`, `join(id)`,
  `sleep(millis)` — over `sys.threadCreate` / `sys.threadJoin`.

<details>
<summary>Example: `Thread.spawn` / `Thread.join`</summary>

```flint
// build: flintc src/stdlib/*.flint s14_thread.flint -o s14_thread && ./s14_thread
import std.Thread;
int worker(int k) {
    return k * 2;
}
int main() {
    print(Thread.nCpu() > 0 ? 1 : 0);
    print("\n");
    int id = Thread.spawn(@worker, 21);
    print(id);
    print("\n");
    print(Thread.join(id));
    print("\n");
    return 0;
}
```

Expected output (thread ids are 0-based; the worker returns 21 × 2):

```
1
0
42
```

</details>

- **`Sync`** (`import std.Sync`): `lock(*int m)`, `unlock(*int m)`,
  `cas(*int x, expect, value)`, `nanosleep(sec, nsec)` — over
  `sys.mutexLock` / `sys.mutexUnlock` / `sys.atomicCas` / `sys.nanosleep`.

<details>
<summary>Example: `Sync.lock` / `unlock` / `cas` / `nanosleep`</summary>

```flint
// build: flintc src/stdlib/*.flint s14_sync.flint -o s14_sync && ./s14_sync
import std.Sync;
int main() {
    int m = 0;
    Sync.lock(&m);
    print(m == 1 ? 1 : 0);
    print("\n");
    Sync.unlock(&m);
    print(m == 0 ? 1 : 0);
    print("\n");
    int val = 5;
    int r = Sync.cas(&val, 5, 10);
    print(r == 1 ? 1 : 0);
    print("\n");
    print(val == 10 ? 1 : 0);
    print("\n");
    Sync.nanosleep(0, 1000000);
    print("slept");
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
1
1
slept
```

</details>

- **Raw primitives:** `sys.clone`, `sys.futex`, `sys.nanosleep`.

<details>
<summary>Example: `sys.nanosleep`</summary>

```flint
// build: flintc s14_raw.flint -o s14_raw && ./s14_raw
int main() {
    int t0 = time.millis();
    sys.nanosleep(0, 1000000);
    int t1 = time.millis();
    print(t1 >= t0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
```

</details>

---

## 15. Fixed-point math (Q48.16)

No floating point — a **Q48.16 fixed-point** numeric model: a 64-bit int whose
lower 16 bits are the fraction (0.5 == 32768; 1.0 == 65536). `math.*` builtins
operate on these values:

- `math.fadd(a, b)`, `math.fsub(a, b)`, `math.fmul(a, b)`, `math.fdiv(a, b)`

<details>
<summary>Example: `fadd` / `fsub` / `fmul` / `fdiv`</summary>

```flint
// build: flintc s15_ops.flint -o s15_ops && ./s15_ops
int main() {
    int a = math.itof(3);
    int b = math.itof(2);
    print(math.ftoi(math.fadd(a, b)));
    print("\n");
    print(math.ftoi(math.fsub(a, b)));
    print("\n");
    print(math.ftoi(math.fmul(a, b)));
    print("\n");
    print(math.ftoi(math.fdiv(math.fmul(a, b), b)));
    print("\n");
    return 0;
}
```

Expected output:

```
5
1
6
3
```

</details>

- `math.itof(n)` — int → fixed-point; `math.ftoi(f)` — fixed-point → int
  (rounds half-up).

<details>
<summary>Example: `itof` / `ftoi` (half-up rounding)</summary>

```flint
// build: flintc s15_conv.flint -o s15_conv && ./s15_conv
int main() {
    int f = math.itof(3);
    print(math.ftoi(f));
    print("\n");
    int half = 32768;
    print(math.ftoi(half));
    print("\n");
    int p15 = 98304;
    print(math.ftoi(p15));
    print("\n");
    return 0;
}
```

Expected output (0.5 → 1 and 1.5 → 2, half-up):

```
3
1
2
```

</details>

- `math.fcmp(a, b)` — returns `< 0`, `0`, `> 0` (like `str.cmp`).

<details>
<summary>Example: `fcmp`</summary>

```flint
// build: flintc s15_fcmp.flint -o s15_fcmp && ./s15_fcmp
int main() {
    int a = math.itof(3);
    int b = math.itof(2);
    print(math.fcmp(a, b) > 0 ? 1 : 0);
    print("\n");
    print(math.fcmp(b, a) < 0 ? 1 : 0);
    print("\n");
    print(math.fcmp(a, a) == 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
1
```

</details>

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

<details>
<summary>Example: `sys` namespace</summary>

Note: `sys.brk` takes one argument (the new break; `0` queries it).

```flint
// build: flintc s16_sys.flint -o s16_sys && ./s16_sys
int main() {
    int fd = sys.open("/dev/null", 2, 0);
    print(fd >= 0 ? 1 : 0);
    print("\n");
    string msg = "hi";
    sys.write(fd, msg, strlen(msg));
    sys.close(fd);
    int brk = sys.brk(0);
    print(brk > 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
```

</details>

<details>
<summary>Example: `str` namespace</summary>

```flint
// build: flintc s16_str.flint -o s16_str && ./s16_str
int main() {
    print(str.cmp("abc", "abd") < 0 ? 1 : 0);
    print("\n");
    print(str.concat("foo", "bar"));
    print("\n");
    print(str.concati("n=", 42));
    print("\n");
    string s = "hello";
    string c = str.copy(s);
    print(c);
    print("\n");
    print(str.itoa(123));
    print("\n");
    print(str.len("abc"));
    print("\n");
    print(str.replace("a-b", "-", "_"));
    print("\n");
    print(str.substring("hello", 1, 3));
    print("\n");
    return 0;
}
```

Expected output:

```
1
foobar
n=42
hello
123
3
a_b
ell
```

</details>

<details>
<summary>Example: `math` namespace (fixed-point)</summary>

```flint
// build: flintc s16_math.flint -o s16_math && ./s16_math
int main() {
    int a = math.itof(3);
    int b = math.itof(2);
    print(math.ftoi(math.fadd(a, b)));
    print("\n");
    print(math.ftoi(math.fmul(a, b)));
    print("\n");
    print(math.fcmp(a, b) > 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
5
6
1
```

</details>

<details>
<summary>Example: `mem` namespace</summary>

```flint
// build: flintc s16_mem.flint -o s16_mem && ./s16_mem
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(5);
    mem.retain(a);
    mem.release(a);
    print(a.v);
    print("\n");
    int r = mem.retainVal(0);
    print(r == 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output (`retainVal` is null-guarded and returns its argument, so
`retainVal(0)` yields 0):

```
5
1
```

</details>

<details>
<summary>Example: `io` namespace (`print` / `println`)</summary>

```flint
// build: flintc s16_io.flint -o s16_io && ./s16_io
int main() {
    print("no newline");
    print("\n");
    println("with newline");
    println(42);
    return 0;
}
```

Expected output:

```
no newline
with newline
42
```

</details>

<details>
<summary>Example: `conv` namespace</summary>

```flint
// build: flintc s16_conv.flint -o s16_conv && ./s16_conv
int main() {
    print(conv.atoi("123"));
    print("\n");
    print(conv.atoi("-7"));
    print("\n");
    return 0;
}
```

Expected output:

```
123
-7
```

</details>

<details>
<summary>Example: `time` namespace</summary>

```flint
// build: flintc s16_time.flint -o s16_time && ./s16_time
int main() {
    int t = time.millis();
    print(t > 0 ? 1 : 0);
    print("\n");
    int n = time.now();
    print(n > 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
```

</details>

<details>
<summary>Example: `rand` namespace</summary>

Note: `rand.range(lo, hi)` divides by `hi - lo`, so `rand.range(n, n)` is a
division by zero (a floating-point exception).

```flint
// build: flintc s16_rand.flint -o s16_rand && ./s16_rand
int main() {
    int r = rand.range(1, 2);
    print(r == 1 ? 1 : 0);
    print("\n");
    int n = rand.next();
    print(n == n ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
```

</details>

<details>
<summary>Example: `env` namespace</summary>

```flint
// build: flintc s16_env.flint -o s16_env && ./s16_env
int main() {
    string p = env.get("PATH");
    print(p != null ? 1 : 0);
    print("\n");
    string none = env.get("FLINT_NO_SUCH_VAR_123");
    print(none == null ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
```

</details>

<details>
<summary>Example: `log` namespace (tagged lines)</summary>

```flint
// build: flintc s16_log.flint -o s16_log && ./s16_log 2>&1
int main() {
    log.info("hello");
    log.warn("careful");
    log.error("bad");
    log.debug("dbg");
    return 0;
}
```

Expected output (run with `2>&1`; `[WARN]` and `[ERROR]` lines come from
stderr, the others from stdout):

```
[INFO] hello[WARN] careful
[ERROR] bad
[DEBUG] dbg
```

Note: `log.info` and `log.debug` write to stdout and add **no** trailing
newline; `log.warn` and `log.error` write to stderr and add one. That is why
`[INFO] hello` and `[WARN] careful` share a line above.

</details>

<details>
<summary>Example: `b64` namespace</summary>

```flint
// build: flintc s16_b64.flint -o s16_b64 && ./s16_b64
int main() {
    print(b64.encode("hi"));
    print("\n");
    print(b64.decode(b64.encode("hi")));
    print("\n");
    return 0;
}
```

Expected output:

```
aGk=
hi
```

</details>

<details>
<summary>Example: `json` namespace</summary>

```flint
// build: flintc s16_json.flint -o s16_json && ./s16_json
int main() {
    string j = "{\"a\": 5, \"b\": \"x\"}";
    print(json.geti(j, "a"));
    print("\n");
    print(json.get(j, "b"));
    print("\n");
    return 0;
}
```

Expected output:

```
5
x
```

</details>

Notes:

- `print` / `println` each take a string or an int (an int argument is formatted
  in decimal) and write to stdout; `println` adds a trailing newline, `print`
  does not.

<details>
<summary>Example: `print` vs `println` (string and int arguments)</summary>

```flint
// build: flintc s16_note_print.flint -o s16_note_print && ./s16_note_print
int main() {
    print("abc");
    print(123);
    print("\n");
    println("def");
    println(456);
    return 0;
}
```

Expected output (`print` adds no newline, so `abc` and `123` share a line):

```
abc123
def
456
```

</details>

- `log.*` writes tagged lines — `log.info` to stdout, `log.warn` to stderr.

<details>
<summary>Example: `log.*` stream split (stdout vs stderr)</summary>

```flint
// build: flintc s16_log.flint -o s16_log && ./s16_log 2>err.txt; echo; echo "-- stderr: --"; cat err.txt
int main() {
    log.info("hello");
    log.warn("careful");
    log.error("bad");
    log.debug("dbg");
    return 0;
}
```

Expected output (with stderr redirected to `err.txt`):

```
[INFO] hello[DEBUG] dbg
-- stderr: --
[WARN] careful
[ERROR] bad
```

Note: the stdout lines carry no trailing newline, so the `echo` above adds
the line break before `-- stderr: --`.

</details>

- `str.format(fmt, a, b, c, d)` is a varargs-style formatter supporting `%d`
  (int), `%s` (*byte), `%b` (int as bool) placeholders.

<details>
<summary>Example: `str.format` (5 arguments: fmt + 4 values)</summary>

```flint
// build: flintc s16_note_format.flint -o s16_note_format && ./s16_note_format
int main() {
    print(str.format("n=%d s=%s b=%b x=%d\n", 42, "hi", 1, 7));
    return 0;
}
```

Expected output:

```
n=42 s=hi b=1 x=7
```

</details>

- `sys.syscall(num, a1, a2, a3, a4, a5)` is the generic syscall escape hatch
  (the 4th syscall argument arrives in `%rcx` and is moved to `%r10`).

<details>
<summary>Example: `sys.syscall` (getpid = 39)</summary>

```flint
// build: flintc s16_note_syscall.flint -o s16_note_syscall && ./s16_note_syscall
int main() {
    int r = sys.syscall(39, 1, 0, 0, 0, 0);
    print(r > 0 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
```

</details>

- `sys.mmap(addr, len, prot, flags, fd, offset)` — `PROT_READ|PROT_WRITE = 3`;
  `MAP_SHARED = 1`, `MAP_PRIVATE|MAP_ANON = 0x22`.

<details>
<summary>Example: `sys.mmap` / `sys.munmap`</summary>

```flint
// build: flintc s16_note_mmap.flint -o s16_note_mmap && ./s16_note_mmap
int main() {
    int p = sys.mmap(0, 4096, 3, 0x22, -1, 0);
    print(p >= 0 ? 1 : 0);
    print("\n");
    sys.munmap(p, 4096);
    print("unmapped");
    print("\n");
    return 0;
}
```

Expected output:

```
1
unmapped
```

</details>

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

<details>
<summary>Example: `Collection` (via `List`, its concrete subclass)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_collection.flint -o s17_collection && ./s17_collection
import std.List;
int main() {
    List<int> l = new List<int>();
    l.add(1);
    l.add(2);
    print(l.size());
    print("\n");
    print(l.isEmpty());
    print("\n");
    l.clear();
    print(l.size());
    print("\n");
    return 0;
}
```

Expected output:

```
2
0
0
```

</details>

<details>
<summary>Example: `List` (sort, reverse, contains, join, remove)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_list.flint -o s17_list && ./s17_list
import std.List;
int main() {
    List<int> l = new List<int>();
    l.add(3);
    l.add(1);
    l.add(2);
    l.sort();
    print(l.get(0));
    print("\n");
    print(l.get(1));
    print("\n");
    print(l.get(2));
    print("\n");
    l.reverse();
    print(l.get(0));
    print("\n");
    print(l.contains(2) ? 1 : 0);
    print("\n");
    print(l.join(","));
    print("\n");
    l.remove(1);
    print(l.size());
    print("\n");
    return 0;
}
```

Expected output:

```
1
2
3
3
1
3,2,1
2
```

</details>

<details>
<summary>Example: `Queue` (FIFO ring)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_queue.flint -o s17_queue && ./s17_queue
import std.Queue;
int main() {
    Queue<int> q = new Queue<int>();
    q.push(10);
    q.push(20);
    q.push(30);
    print(q.size());
    print("\n");
    print(q.peek());
    print("\n");
    print(q.pop());
    print("\n");
    print(q.pop());
    print("\n");
    q.push(40);
    print(q.peek());
    print("\n");
    print(q.size());
    print("\n");
    return 0;
}
```

Expected output:

```
3
10
10
20
30
2
```

</details>

<details>
<summary>Example: `HashSet` (duplicates collapse)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_hashset.flint -o s17_hashset && ./s17_hashset
import std.HashSet;
int main() {
    HashSet<int> s = new HashSet<int>();
    s.add(5);
    s.add(5);
    s.add(9);
    print(s.size());
    print("\n");
    print(s.contains(5) ? 1 : 0);
    print("\n");
    print(s.contains(7) ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
2
1
0
```

</details>

<details>
<summary>Example: `HashMap`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_hashmap.flint -o s17_hashmap && ./s17_hashmap
import std.HashMap;
int main() {
    HashMap<string, int> m = new HashMap<string, int>();
    m.put("a", 1);
    m.put("b", 2);
    print(m.get("a"));
    print("\n");
    print(m.get("b"));
    print("\n");
    print(m.contains("a") ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
2
1
```

</details>

<details>
<summary>Example: `CopyOnWriteList` (set, indexOf, remove)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_cowlist.flint -o s17_cowlist && ./s17_cowlist
import std.CopyOnWriteList;
int main() {
    CopyOnWriteList<int> l = new CopyOnWriteList<int>();
    l.add(1);
    l.add(2);
    l.set(0, 10);
    print(l.get(0));
    print("\n");
    print(l.indexOf(2));
    print("\n");
    l.remove(1);
    print(l.size());
    print("\n");
    return 0;
}
```

Expected output:

```
10
1
1
```

</details>

<details>
<summary>Example: `map` / `reduce` / `forEach` (+ `ReduceBox` under the hood)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_mapreduce.flint -o s17_mapreduce && ./s17_mapreduce
import std.List;
int main() {
    List<int> xs = new List<int>();
    xs.add(1);
    xs.add(2);
    xs.add(3);
    List<int> ys = std.map<int, int>(xs, (int x) -> int { return x * 2; });
    int sum = 0;
    for (int i = 0; i < ys.size(); i = i + 1) {
        sum = sum + ys.get(i);
    }
    print(sum);
    print("\n");
    int total = std.reduce<int, int>(xs, (int acc, int x) -> int { return acc + x; }, 0);
    print(total);
    print("\n");
    std.forEach<int>(xs, (int x) -> void { print(x); print(" "); });
    print("\n");
    return 0;
}
```

Expected output (the 3rd line is `1 2 3 ` with a trailing space):

```
12
6
1 2 3 
```

</details>

<details>
<summary>Example: `Arrays.filter` (raw `int[]`)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_arrays.flint -o s17_arrays && ./s17_arrays
import std.Arrays;
int main() {
    int a[] = [1, 2, 3, 4, 5];
    int ev[] = Arrays.filter(a, (int x) -> int { return x % 2 == 0; });
    int sum = 0;
    for (int i = 0; i < len(ev); i = i + 1) {
        sum = sum + ev[i];
    }
    print(sum);
    print("\n");
    return 0;
}
```

Expected output:

```
6
```

</details>

<details>
<summary>Example: `Bit`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_bit.flint -o s17_bit && ./s17_bit
import std.Bit;
int main() {
    print(Bit.popcount(0xF));
    print("\n");
    print(Bit.isPowerOfTwo(16) ? 1 : 0);
    print("\n");
    int x = 0;
    x = Bit.setBit(x, 3);
    print(Bit.testBit(x, 3) ? 1 : 0);
    print("\n");
    print(Bit.byteSwap(0x1234));
    print("\n");
    return 0;
}
```

Expected output (`byteSwap` is a full 64-bit swap: 0x…00001234 → 0x3412000000000000):

```
4
1
1
3752061439553044480
```

</details>

<details>
<summary>Example: `Checksum`</summary>

Note: `Checksum.crc32` currently segfaults (the table path in
`checksum.flint` is buggy); `sum32` and `djb2` work.

```flint
// build: flintc src/stdlib/*.flint s17_checksum.flint -o s17_checksum && ./s17_checksum
import std.Checksum;
int main() {
    string s = "hello";
    print(Checksum.sum32(s));
    print("\n");
    print(Checksum.djb2(s));
    print("\n");
    return 0;
}
```

Expected output:

```
532
210714636441
```

</details>

<details>
<summary>Example: `File` (write, exists, read, delete)</summary>

Note: `File.size` currently returns 0 for every file (it reads the wrong
`struct stat` offset); the size below comes from `strlen(File.readAll(p))`.

```flint
// build: flintc src/stdlib/*.flint s17_file.flint -o s17_file && ./s17_file
import std.File;
int main() {
    string p = "/tmp/flint_feat_file.txt";
    File.writeAll(p, "hello file");
    print(File.exists(p) ? 1 : 0);
    print("\n");
    string data = File.readAll(p);
    print(strlen(data));
    print("\n");
    print(data);
    print("\n");
    File.delete(p);
    print(File.exists(p) ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
10
hello file
0
```

</details>

<details>
<summary>Example: `Path`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_path.flint -o s17_path && ./s17_path
import std.Path;
int main() {
    print(Path.base("/a/b/c.txt"));
    print("\n");
    print(Path.dir("/a/b/c.txt"));
    print("\n");
    print(Path.ext("/a/b/c.txt"));
    print("\n");
    print(Path.join("/a", "b.txt"));
    print("\n");
    print(Path.isAbsolute("/a") ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
c.txt
/a/b
txt
/a/b.txt
1
```

</details>

<details>
<summary>Example: `Rand` (seeded, deterministic)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_rand.flint -o s17_rand && ./s17_rand
import std.Rand;
int main() {
    Rand.init(42);
    int a = Rand.next();
    Rand.init(42);
    int b = Rand.next();
    print(a == b ? 1 : 0);
    print("\n");
    int r = Rand.range(5, 5);
    print(r == 5 ? 1 : 0);
    print("\n");
    print(Rand.coin() <= 1 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
1
```

</details>

<details>
<summary>Example: `Sort`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_sort.flint -o s17_sort && ./s17_sort
import std.Sort;
int main() {
    int a[] = [3, 1, 2];
    Sort.sort(a);
    print(a[0] + a[1] * 10 + a[2] * 100);
    print("\n");
    Sort.reverse(a);
    print(a[0]);
    print("\n");
    print(Sort.max(a));
    print("\n");
    print(Sort.min(a));
    print("\n");
    print(Sort.sum(a));
    print("\n");
    return 0;
}
```

Expected output (sorted `1 2 3` → 1+20+300; reversed `3 2 1`):

```
321
3
3
1
6
```

</details>

<details>
<summary>Example: `Str`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_str.flint -o s17_str && ./s17_str
import std.Str;
int main() {
    print(Str.upper("hi"));
    print("\n");
    print(Str.lower("HI"));
    print("\n");
    print(Str.reverse("ab"));
    print("\n");
    print(Str.trim("  x  "));
    print("\n");
    print(Str.count("aba", "a"));
    print("\n");
    print(Str.startsWith("abc", "ab") ? 1 : 0);
    print("\n");
    print(Str.repeat("ab", 2));
    print("\n");
    return 0;
}
```

Expected output:

```
HI
hi
ba
x
2
1
abab
```

</details>

<details>
<summary>Example: `Num`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_num.flint -o s17_num && ./s17_num
import std.Num;
int main() {
    print(Num.toString(123));
    print("\n");
    print(Num.toHex(255));
    print("\n");
    print(Num.fromHex("ff"));
    print("\n");
    print(Num.sumDigits(123));
    print("\n");
    print(Num.isPalindrome(121) ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output (`toHex` is lowercase; use `toUpperHex` for capitals):

```
123
ff
255
6
1
```

</details>

<details>
<summary>Example: `Math`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_math.flint -o s17_math && ./s17_math
import std.Math;
int main() {
    print(Math.abs(-5));
    print("\n");
    print(Math.min(3, 7));
    print("\n");
    print(Math.max(3, 7));
    print("\n");
    print(Math.gcd(12, 18));
    print("\n");
    print(Math.factorial(5));
    print("\n");
    print(Math.isPrime(7) ? 1 : 0);
    print("\n");
    print(Math.sqrt(16));
    print("\n");
    return 0;
}
```

Expected output:

```
5
3
7
6
120
1
4
```

</details>

<details>
<summary>Example: `Mem`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_mem.flint -o s17_mem && ./s17_mem
import std.Mem;
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int main() {
    Box a = new Box(5);
    Mem.retain(a);
    Mem.release(a);
    print(a.v);
    print("\n");
    return 0;
}
```

Expected output:

```
5
```

</details>

<details>
<summary>Example: `Sync`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_sync.flint -o s17_sync && ./s17_sync
import std.Sync;
int main() {
    int m = 0;
    Sync.lock(&m);
    print(m == 1 ? 1 : 0);
    print("\n");
    Sync.unlock(&m);
    print(m == 0 ? 1 : 0);
    print("\n");
    int val = 5;
    int r = Sync.cas(&val, 5, 10);
    print(r == 1 ? 1 : 0);
    print("\n");
    print(val == 10 ? 1 : 0);
    print("\n");
    return 0;
}
```

Expected output:

```
1
1
1
1
```

</details>

<details>
<summary>Example: `Thread`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_thread.flint -o s17_thread && ./s17_thread
import std.Thread;
int worker(int k) {
    return k * 2;
}
int main() {
    print(Thread.nCpu() > 0 ? 1 : 0);
    print("\n");
    int id = Thread.spawn(@worker, 21);
    print(id);
    print("\n");
    print(Thread.join(id));
    print("\n");
    return 0;
}
```

Expected output (ids are 0-based):

```
1
0
42
```

</details>

<details>
<summary>Example: `Task`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_task.flint -o s17_task && ./s17_task
import std.Task;
async int add(int a, int b) {
    return a + b;
}
int main() {
    Task t = add(20, 22);
    print(t.join());
    print("\n");
    return 0;
}
```

Expected output:

```
42
```

</details>

<details>
<summary>Example: `Time`</summary>

```flint
// build: flintc src/stdlib/*.flint s17_time.flint -o s17_time && ./s17_time
import std.Time;
int main() {
    print(Time.millis() > 0 ? 1 : 0);
    print("\n");
    print(Time.seconds() > 0 ? 1 : 0);
    print("\n");
    string d = Time.date();
    print(str.len(d));
    print("\n");
    return 0;
}
```

Expected output (`date()` is `YYYY-MM-DD HH:MM:SS` — 19 characters):

```
1
1
19
```

</details>

<details>
<summary>Example: `Socket` (TCP loopback in one process)</summary>

Note: `Socket.connectHost` returns the connected socket fd — use it (not a
separate `stream()` fd) for send/recv.

```flint
// build: flintc src/stdlib/*.flint s17_socket.flint -o s17_socket && ./s17_socket
import std.Socket;
import std.Thread;
int server(int port) {
    int fd = Socket.stream();
    Socket.bindPort(fd, port);
    Socket.listen(fd, 1);
    int c = Socket.accept(fd);
    string msg = Socket.recv(c);
    Socket.sendAll(c, "got:" + msg);
    Socket.close(c);
    Socket.close(fd);
    return 0;
}
int main() {
    int port = 45678;
    int id = Thread.spawn(@server, port);
    Thread.sleep(300);
    int fd = Socket.connectHost(127, 0, 0, 1, port);
    Socket.sendAll(fd, "ping");
    string r = Socket.recv(fd);
    print(r);
    print("\n");
    Socket.close(fd);
    Thread.join(id);
    return 0;
}
```

Expected output:

```
got:ping
```

</details>

<details>
<summary>Example: `Image` (RGBA pixels, invert, PPM export)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_image.flint -o s17_image && ./s17_image
import std.Image;
int main() {
    Image im = new Image(4, 4);
    im.fill(Image.rgb(255, 0, 0));
    im.set(1, 1, Image.rgb(0, 255, 0));
    print(im.r(1, 1));
    print("\n");
    print(im.g(1, 1));
    print("\n");
    im.invert();
    print(im.r(0, 0));
    print("\n");
    im.toPpm("/tmp/flint_feat.ppm");
    print("saved");
    print("\n");
    return 0;
}
```

Expected output:

```
0
255
0
saved
```

</details>

<details>
<summary>Example: `Window` (Wayland — open, present, close)</summary>

Requires a Wayland compositor; without one the program prints `no display`
and exits. Note: this build segfaults at process exit (after `closed` is
printed) — a runtime teardown bug.

```flint
// build: flintc src/stdlib/*.flint s17_window.flint -o s17_window && ./s17_window
import std.Window;
import std.Image;
int main() {
    Window w = Window.open(320, 200, "flint feat");
    if (w == null) {
        print("no display");
        print("\n");
        return 0;
    }
    print("opened");
    print("\n");
    Image im = new Image(320, 200);
    im.fill(Image.rgb(10, 10, 20));
    int k = 0;
    while (w.running() == 1 && k < 3) {
        w.present(im);
        sys.nanosleep(0, 5000000);
        k = k + 1;
    }
    w.close();
    print("closed");
    print("\n");
    return 0;
}
```

Expected output (with a compositor; then a segfault at exit):

```
opened
closed
```

</details>

<details>
<summary>Example: `App` (event-loop host with `OnSetup`/`OnDraw` handlers)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_app.flint -o s17_app && ./s17_app
import std.App;
import std.OnSetup;
import std.OnDraw;
import std.OnClose;
import std.Image;
class Demo implements OnSetup, OnDraw {
    App app;
    int frames;
    void setup() {
        print("setup");
        print("\n");
    }
    void draw(Image c) {
        c.fill(Image.rgb(10, 10, 20));
        this.frames = this.frames + 1;
        if (this.frames >= 3) {
            this.app.stop();
        }
    }
}
int main() {
    Demo d = new Demo();
    d.app = App.open(320, 200, "flint app");
    if (d.app == null) {
        print("no display");
        print("\n");
        return 0;
    }
    d.app.setup += d;
    d.app.draw += d;
    int rc = d.app.run();
    print("frames=" + str.itoa(d.frames));
    print("\n");
    print(rc);
    print("\n");
    return 0;
}
```

Expected output (requires a Wayland compositor; `run()` returns 0):

```
setup
frames=3
0
```

</details>

<details>
<summary>Example: event-handler interfaces (`OnSetup`, `OnDraw`, `OnClose`)</summary>

Note: the `OnClose` handler runs on a compositor close event, not on
`stop()` — so only `setup` fires in this run.

```flint
// build: flintc src/stdlib/*.flint s17_handlers.flint -o s17_handlers && ./s17_handlers
import std.App;
import std.OnSetup;
import std.OnDraw;
import std.OnClose;
import std.Image;
class H implements OnSetup, OnDraw, OnClose {
    App app;
    int frames;
    void setup() {
        print("setup");
        print("\n");
    }
    void draw(Image c) {
        c.fill(Image.rgb(20, 20, 20));
        this.frames = this.frames + 1;
        if (this.frames >= 3) {
            this.app.stop();
        }
    }
    void close() {
        print("close handler");
        print("\n");
    }
}
int main() {
    H h = new H();
    h.app = App.open(320, 200, "flint handlers");
    if (h.app == null) {
        print("no display");
        print("\n");
        return 0;
    }
    h.app.setup += h;
    h.app.draw += h;
    h.app.close += h;
    h.app.run();
    print("done");
    print("\n");
    return 0;
}
```

Expected output (requires a Wayland compositor):

```
setup
done
```

</details>

<details>
<summary>Example: `Widget` + subclasses (via `std.Gui` — `Label`, `TextBox`)</summary>

```flint
// build: flintc src/stdlib/*.flint s17_widget.flint -o s17_widget && ./s17_widget
import std.Gui;
import std.Image;
import std.Label;
import std.TextBox;
import std.OnSetup;
import std.OnDraw;
class WApp implements OnSetup, OnDraw {
    Gui gui;
    TextBox box;
    int frames;
    void setup() {
        Label title = new Label(16, 12, 240, 16, "widget demo");
        this.box = new TextBox(16, 48, 240, 30, 32);
        this.gui.addLabel(title);
        this.gui.addBox(this.box);
    }
    void draw(Image c) {
        c.fill(Image.rgb(16, 16, 32));
        this.frames = this.frames + 1;
        if (this.box.entered() == 1) {
            print("typed [");
            print(this.box.text());
            print("]\n");
            this.box.clearEntered();
        }
        if (this.frames >= 3) {
            this.gui.stop();
        }
    }
}
int main() {
    WApp app = new WApp();
    app.gui = Gui.open(480, 200, "flint widget");
    if (app.gui == null) {
        print("no display");
        print("\n");
        return 0;
    }
    print("opened ok");
    print("\n");
    app.gui.setup += app;
    app.gui.draw += app;
    int rc = app.gui.run();
    print("frames=" + str.itoa(app.frames));
    print("\n");
    print(rc);
    print("\n");
    return 0;
}
```

Expected output (requires a Wayland compositor):

```
opened ok
frames=3
0
```

</details>
---

## 18. Modules & multi-file

- **Packages:** `package a.b.c;` at the top of a file (dotted path, no leading
  dot; empty = default package).

<details>
<summary>Example: packages (two files, `package` + qualified use)</summary>

`util.flint`:

```flint
package com.example;
class Util {
    public Util() {
    }
    public int doubleIt(int x) {
        return x * 2;
    }
}
```

`main.flint`:

```flint
import com.example.Util;
int main() {
    Util u = new Util();
    print(u.doubleIt(21));
    print("\n");
    return 0;
}
```

```sh
# build: flintc util.flint main.flint -o out && ./out
```

Expected output:

```
42
```

</details>

- **Imports:** `import a.b.Foo;` (a fully qualified name) or
  `import a.b.*;` (wildcard). Collected across all input files.

<details>
<summary>Example: wildcard import</summary>

`wildcard_main.flint` (with the same `util.flint` as above):

```flint
import com.example.*;
int main() {
    Util u = new Util();
    print(u.doubleIt(21));
    print("\n");
    return 0;
}
```

```sh
# build: flintc util.flint wildcard_main.flint -o out && ./out
```

Expected output:

```
42
```

</details>

- **Multi-file:** `flintc a.flint b.flint -o out` — files are concatenated
  (each keeps its own `package` header).

<details>
<summary>Example: multi-file (default package)</summary>

`a.flint`:

```flint
int helper(int x) {
    return x + 1;
}
```

`b.flint`:

```flint
int main() {
    print(helper(41));
    print("\n");
    return 0;
}
```

```sh
# build: flintc a.flint b.flint -o out && ./out
```

Expected output:

```
42
```

</details>

- **Manifest:** `flintc --manifest box.toml -o out` — read source files from a
  TOML manifest (`sources = ["util.flint", "main.flint"]`).

<details>
<summary>Example: `--manifest`</summary>

`flint.toml`:

```toml
sources = ["a.flint", "b.flint"]
```

(`a.flint` / `b.flint` as in the multi-file example above.)

```sh
# build: flintc --manifest flint.toml -o out && ./out
```

Expected output:

```
42
```

</details>

- **Directory:** `flintc --dir src -o out` — compile every `.flint` in a
  directory (sorted).

<details>
<summary>Example: `--dir`</summary>

`dir/a.flint` and `dir/b.flint` (as in the multi-file example above).

```sh
# build: flintc --dir dir -o out && ./out
```

Expected output:

```
42
```

</details>

- **Top-level constants:** `const NAME = <value>;` (int, bool, or string),
  scoped by `package`; inlined at use sites by the monomorphizer.

<details>
<summary>Example: top-level constants</summary>

```flint
// build: flintc s18_const.flint -o s18_const && ./s18_const
const ANSWER = 42;
const NAME = "flint";
const ON = true;
int main() {
    print(ANSWER);
    print("\n");
    print(NAME);
    print("\n");
    print(ON);
    print("\n");
    return 0;
}
```

Expected output:

```
42
flint
1
```

</details>

- **Type aliases:** `type NAME = <type>;` (top-level), resolved by the parser.

<details>
<summary>Example: type alias (top-level)</summary>

Note: the compiler keyword is `alias` — `type` is rejected at top level.

```flint
// build: flintc s18_alias.flint -o s18_alias && ./s18_alias
alias Big = int;
int main() {
    Big x = 5;
    print(x * 2);
    print("\n");
    return 0;
}
```

Expected output:

```
10
```

</details>

---

## 19. The compiler & CLI

**Pipeline:** `lexer → parser → desugar (lambdas) → monomorphize → ownership
check → codegen → .s → as → ld -e _start`.

<details>
<summary>Example: the pipeline (generated assembly for a small program)</summary>

```flint
// build: flintc -S hello.flint        (stops after codegen; prints the .s)
int main() {
    print("hello");
    print("\n");
    return 0;
}
```

Expected output (first lines of the generated assembly):

```
.section .text
main:
.type main, @function
	push %rbp
	mov %rsp, %rbp
	lea .Lstr0(%rip), %rax
	push %rax
	pop %rdi
	call flint_printstr
	movq $0, %rax
	push %rax
	pop %rax
```

</details>

- **middle/** — lambda desugaring (lift a lambda to a function taking its
  captures; replace the node with a closure); monomorphization (`ensure`,
  `expand`, `infer`, `resolve`, `resolve_expr`); ownership checking
  (`ownership.rs`).

<details>
<summary>Example: lambda desugaring (the lifted function in the assembly)</summary>

```flint
// build: flintc -S lambda.flint | grep -n lambda
int main() {
    int f = (int x) -> int { return x * 2; };
    print(sys.fnCall2(f, 21));
    print("\n");
    return 0;
}
```

Expected output — the lambda is lifted to a top-level function
`main__lambda0` whose first parameter is the capture block:

```
11:	lea main__lambda0(%rip), %rax
49:main__lambda0:
50:.type main__lambda0, @function
72:.size main__lambda0, .-main__lambda0
```

</details>

- **backend/** — code generation (`codegen/`: `accessor`, `binop`, `builtin`,
  `call`, `ctx`, `expr`, `func`, `lvalue`, `new`, `stmt`) plus `escape.rs`
  (stack-vs-heap escape analysis), `layout.rs`, and `release.rs`.

<details>
<summary>Example: escape analysis (refcount calls in the assembly)</summary>

```flint
// build: flintc -S classy.flint | grep -n "flint_retain\|flint_release\|flint_alloc"
class Box {
    int v;
    public Box(int x) {
        this.v = x;
    }
}
int use(Box b) {
    return b.v;
}
int main() {
    Box b = new Box(7);
    print(use(b));
    print("\n");
    return 0;
}
```

Expected output — `b` escapes (passed to `use`), so it is heap-allocated and
refcounted:

```
15:	call flint_release_Box
21:	call flint_release_Box
34:	call flint_alloc
36:	lea flint_release_Box_at0(%rip), %rdi
49:	call flint_release_Box
54:	call flint_retain
73:	call flint_release_Box
79:	call flint_release_Box
107:.globl flint_release_Box
108:.type flint_release_Box, @function
109:flint_release_Box:
118:.globl flint_release_Box_at0
120:flint_release_Box_at0:
130:.size flint_release_Box, .-flint_release_Box
```

</details>

- **intrinsics.s** — the freestanding runtime, embedded at compile time and
  linked into every binary (raw `syscall`s, heap, refcount, threads, sockets,
  print helpers, the `destroy()` hook).

<details>
<summary>Example: the runtime is linked into every binary</summary>

```sh
# build: flintc hello.flint -o hello_bin
# then:  nm hello_bin | grep -c flint_
```

Expected output — 107 `flint_*` runtime symbols in the binary (a sample):

```
107
flint_accept
flint_alloc
flint_array_release
flint_atoi
flint_atomic_cas
flint_b64_decode
flint_b64_encode
flint_bind
flint_brk
flint_buf_len
flint_call_ptr
flint_clone
flint_close
flint_connect
flint_cpu_count
flint_cpu_mask
flint_env_get
flint_epoll_create1
flint_epoll_ctl
flint_epoll_wait
flint_exc_check
flint_exc_clear
flint_exception
flint_exc_get
flint_exit
flint_fadd
flint_fcmp
flint_fdiv
flint_fmul
flint_fn_addr
```

</details>

**`flintc` CLI flags:**

- `-o <FILE>` — write output to `FILE` (default: first input without `.flint`).

<details>
<summary>Example: `-o`</summary>

```sh
# build: flintc hello.flint -o hello_bin && ./hello_bin
```

Expected output:

```
hello
```

</details>

- `-S` — stop after generating assembly (print to stdout, or to a file with
  `-o out.s`).

<details>
<summary>Example: `-S`</summary>

```sh
# build: flintc -S hello.flint | head -12
```

Expected output (first lines of the assembly):

```
.section .text
main:
.type main, @function
	push %rbp
	mov %rsp, %rbp
	lea .Lstr0(%rip), %rax
	push %rax
	pop %rdi
	call flint_printstr
	movq $0, %rax
	push %rax
	pop %rax
```

</details>

- `--manifest <FILE>` — read source files from a TOML manifest.

<details>
<summary>Example: `--manifest`</summary>

```sh
# build: flintc --manifest flint.toml -o out && ./out
# (flint.toml: sources = ["a.flint", "b.flint"])
```

Expected output:

```
42
```

</details>

- `--dir <DIR>` — compile every `.flint` in `DIR` (sorted).

<details>
<summary>Example: `--dir`</summary>

```sh
# build: flintc --dir dir -o out && ./out
# (dir/ holds a.flint and b.flint)
```

Expected output:

```
42
```

</details>

- `-h` / `--help` — print usage.

<details>
<summary>Example: `-h`</summary>

```sh
# build: flintc -h 2>&1
```

Expected output (the usage goes to stderr, hence `2>&1`):

```
usage: flintc [OPTIONS] <input.flint>...

Compile one or more Flint files into a freestanding binary. Multiple
files are concatenated (each keeps its own `package` header).

Options:
	-o <FILE>	write output to FILE (default: <first input> without .flint)
	-S		stop after generating assembly (print to stdout)
	--manifest <FILE>	read source files from a TOML manifest
	--dir <DIR>	compile every .flint file in DIR (sorted)
	-h, --help	print this help
```

</details>

**Build & test:**

```sh
cargo build                 # -> target/debug/flintc (release: --release)
bash tests/run_tests.sh     # full suite (expect: PASS=126  FAIL=0)
```

Requires `as` and `ld` (binutils) on the PATH.

<details>
<summary>Example: build & test (current suite output)</summary>

```sh
cargo build
bash tests/run_tests.sh | tail -3
```

Expected output (the suite currently has 135 tests):

```
====================================
PASS=135  FAIL=0
ALL TESTS PASSED
```

</details>

---

## 20. Deliberate departures & v1 limitations

- **`x++` / `++x`** both evaluate to the value **after** the increment (no
  "old value" form) — a departure from C/Java.

<details>
<summary>Example: post-value increment</summary>

```flint
// build: flintc s20_postinc.flint -o s20_postinc && ./s20_postinc
int main() {
    int x = 5;
    print(x++);
    print("\n");
    print(++x);
    print("\n");
    return 0;
}
```

Expected output (both forms yield the value *after* the increment:
`x++` → 6, then `++x` → 7):

```
6
7
```

</details>

- **Flat scope** — no block-level shadowing (re-declaration overwrites).

<details>
<summary>Example: flat scope</summary>

```flint
// build: flintc s20_flatscope.flint -o s20_flatscope && ./s20_flatscope
int main() {
    int v = 10;
    if (1) {
        int v = 20;
        print(v);
    }
    print("\n");
    print(v);
    print("\n");
    return 0;
}
```

Expected output (the inner declaration overwrites `v`; the new value persists):

```
20
20
```

</details>

- **Downcasts are unchecked** (trust the programmer).

<details>
<summary>Example: unchecked downcast (of a non-`Dog` `Animal`)</summary>

```flint
// build: flintc s20_downcast.flint -o s20_downcast && ./s20_downcast
class Animal {
    int id;
    public Animal(int i) {
        this.id = i;
    }
}
class Dog extends Animal {
    int barks;
    public Dog(int i, int b) {
        super(i);
        this.barks = b;
    }
}
int main() {
    Animal a = new Animal(1);
    Dog d = (Dog) a;
    print(d.id);
    print("\n");
    return 0;
}
```

Expected output (no check is performed; `id` is readable because it sits at
the same offset — `d.barks` would read garbage):

```
1
```

</details>

- **Cycles leak** — no weak references.

<details>
<summary>Example: a reference cycle (leaks, but runs fine)</summary>

```flint
// build: flintc s20_cycles.flint -o s20_cycles && ./s20_cycles
class Node {
    Node next;
    public Node() {
        this.next = null;
    }
}
int main() {
    Node a = new Node();
    Node b = new Node();
    a.next = b.move();
    b.next = a.move();
    print("ok");
    print("\n");
    return 0;
}
```

Expected output (the cycle keeps both objects' refcounts above zero, so they
are never freed — but the program runs):

```
ok
```

</details>

- **No `alloc` / `free` / `memcpy`** — typed buffers only; owned values free
  themselves at scope end.

<details>
<summary>Example: `alloc` is a compile error</summary>

```flint
// build: flintc s20_noalloc.flint -o s20_noalloc   (fails to compile)
int main() {
    int p = alloc(16);
    return 0;
}
```

Expected output (compiler error):

```
error[3:13]: alloc() was removed; declare a typed buffer instead (`int buf[n]`, `string s[n]`)
  |     int p = alloc(16);
  |             ^
```

</details>

- **No method-level generics** in v1 (a method's `type_params` is always empty);
  generic free functions are the mechanism for polymorphic calls
  (`std.map<T, U>`, `identity<T>`).

<details>
<summary>Example: a generic method is a compile error</summary>

```flint
// build: flintc s20_nomethgeneric.flint -o s20_nomethgeneric   (fails to compile)
class C {
    int f<T>(T x) {
        return 0;
    }
}
int main() {
    return 0;
}
```

Expected output (compiler error):

```
error[3:10]: expected '(' for method or ';' for field, found Lt
  |     int f<T>(T x) {
  |          ^
```

</details>

- **No generic interfaces, no interface inheritance, no enum associated data** in
  v1.

<details>
<summary>Example: a generic interface is a compile error</summary>

```flint
// build: flintc s20_nogeneriface.flint -o s20_nogeneriface   (fails to compile)
interface I<T> {
    int m(T x);
}
int main() {
    return 0;
}
```

Expected output (compiler error):

```
error[2:12]: expected '{', found Lt
  | interface I<T> {
  |            ^
```

</details>

- **Array rank is untracked** in v1.

<details>
<summary>Example: 2-D array (rank is untracked)</summary>

```flint
// build: flintc s20_arrank.flint -o s20_arrank && ./s20_arrank
int main() {
    int[][] g = [[1, 2], [3, 4]];
    print(len(g));
    print("\n");
    print(g[1][1]);
    print("\n");
    return 0;
}
```

Expected output:

```
2
4
```

</details>

- **At most 32 tasks** in flight at once.

<details>
<summary>Example: 32 concurrent `async` tasks</summary>

```flint
// build: flintc src/stdlib/*.flint s20_32tasks.flint -o s20_32tasks && ./s20_32tasks
import std.Sync;
class State {
    static int started;
    static int done;
    static int lock1;
    static int lock2;
    static int release;
    static int sum;
}
async int work(int k) {
    Sync.lock(&State.lock1);
    State.started = State.started + 1;
    Sync.unlock(&State.lock1);
    while (State.release == 0) {
        Sync.nanosleep(0, 1000000);
    }
    Sync.lock(&State.lock2);
    State.sum = State.sum + k * k;
    State.done = State.done + 1;
    Sync.unlock(&State.lock2);
    return 0;
}
int main() {
    for (int k = 0; k < 32; k = k + 1) {
        work(k);
    }
    while (State.started < 32) {
        Sync.nanosleep(0, 1000000);
    }
    print("32 in flight");
    print("\n");
    State.release = 1;
    while (State.done < 32) {
        Sync.nanosleep(0, 1000000);
    }
    print(State.sum);
    print("\n");
    return 0;
}
```

Expected output (all 32 tasks run at once; Σ k² for k=0..31 = 10416):

```
32 in flight
10416
```

</details>

- **No floating point** — Q48.16 fixed-point only.

<details>
<summary>Example: a float literal is a compile error</summary>

```flint
// build: flintc s20_nofloat.flint -o s20_nofloat   (fails to compile)
int main() {
    int x = 1.5;
    return 0;
}
```

Expected output (compiler error):

```
error[3:15]: expected identifier, found Int(5)
  |     int x = 1.5;
  |               ^
```

</details>
