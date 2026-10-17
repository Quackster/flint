# Flint / flintc

Flint is a small, statically typed systems programming language.

It compiles directly to **x86_64 Linux assembly** and does not use libc. The compiler, `flintc`, is written in Rust with no external dependencies.

📚 **Full documentation:** <https://h4bbo.net/flint>

### Overview

- Source files: `*.flint`
- Target: x86_64 Linux
- Runtime: freestanding, no libc
- Compiler: Rust
- Output: Linux executables or x86_64 assembly

The compiler pipeline is:

```text
lexer → parser → desugar → monomorphize → ownership → codegen → assembly → linker
```

Flint includes a small hand-written runtime in `intrinsics.s`. It provides system calls, memory management, reference counting, threads, sockets, and basic printing.

Collections are part of the standard library, not the runtime.

## Quick start

Clone and build Flint:

```sh
git clone https://github.com/Quackster/flint.git
cd flint

cargo build --release
```

The compiler will be available at:

```text
target/release/flintc
```

You can also build a debug version:

```sh
cargo build
```

### Requirements

Flint needs `as` and `ld` from **binutils**.

Make sure both are available in your `PATH`.

## Compile a program

Compile and run the hello-world example:

```sh
flintc examples/hello.flint -o hello
./hello
```

Output:

```text
hello, world
```

### Generate assembly

To print assembly:

```sh
flintc examples/hello.flint -S
```

To save it to a file:

```sh
flintc examples/hello.flint -o out.s -S
```

## Multiple source files

You can compile multiple files:

```sh
flintc a.flint b.flint -o out
```

You can also use a TOML manifest:

```sh
flintc --manifest box.toml -o out
```

Example `box.toml`:

```toml
sources = ["util.flint", "main.flint"]
```

Or compile every `.flint` file in a directory:

```sh
flintc --dir src -o out
```

Files are compiled in sorted order.

## Command-line options

| Option | Description |
|---|---|
| `-o <FILE>` | Set the output file |
| `-S` | Generate assembly only |
| `--manifest <FILE>` | Use a TOML manifest |
| `--dir <DIR>` | Compile all `.flint` files in a directory |
| `-h`, `--help` | Show help |

If `-o` is not provided, the output name is based on the first input file.

## Tests

Run the Rust tests:

```sh
cargo test
```

Run the full test suite:

```sh
tests/run_tests.sh
```

The test suite covers program execution, assembly output, error messages, and multi-file programs.

## Compiler architecture

The compiler is split into a few main stages:

```text
lexer
  ↓
parser
  ↓
lambda desugaring
  ↓
monomorphization
  ↓
ownership checking
  ↓
code generation
  ↓
assembly
  ↓
as
  ↓
ld
```

### `middle/`

This contains the compiler's middle-end.

**Lambdas**

Lambdas are converted into normal functions with their captured values stored in a closure.

**Generics**

Generic code is expanded into concrete types and functions during monomorphization.

**Ownership**

Flint has compile-time ownership and borrowing checks inspired by Rust.

For example:

```text
T x = y
```

moves `y` into `x`.

Values can be explicitly copied when needed:

```text
x.move()
str.copy(s)
```

Owned values are automatically released when they leave scope.

### `backend/`

The backend turns Flint code into x86_64 assembly.

It handles:

- expressions
- functions
- calls
- operators
- variables
- memory allocation
