#!/usr/bin/env bash
# flintc test harness: execution + golden-asm + error tests.
# Usage: tests/run_tests.sh
set -u
cd "$(dirname "$0")/.."

FLINTC="${FLINTC:-target/debug/flintc}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0
fail=0
ok()   { echo "  PASS  $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL  $1"; fail=$((fail+1)); }

echo "== building flintc =="
if ! cargo build -q 2> "$WORK/build.log"; then
    echo "  FAIL  cargo build"; cat "$WORK/build.log"; exit 1
fi
[ -x "$FLINTC" ] || { echo "  FAIL  $FLINTC not found (run cargo build)"; exit 1; }
echo "  using $FLINTC"

# ---------------------------------------------------------------- execution
# Case tests compile with the whole stdlib so they may `import std.*`.
echo
echo "== execution tests (tests/cases, with stdlib) =="
for src in tests/cases/*.flint; do
    name="$(basename "$src" .flint)"
    exp="tests/cases/expected/$name.out"
    bin="$WORK/$name.bin"
    if ! timeout 20 "$FLINTC" src/stdlib/*.flint "$src" -o "$bin" 2> "$WORK/$name.cerr"; then
        bad "$name (compile)"; sed 's/^/      /' "$WORK/$name.cerr"; continue
    fi
    timeout 20 "$bin" > "$WORK/$name.out" 2>&1
    code=$?
    if [ "$code" -ne 0 ]; then
        bad "$name (exit=$code)"; sed 's/^/      /' "$WORK/$name.out"; continue
    fi
    if [ -f "$exp" ]; then
        if diff -u "$exp" "$WORK/$name.out" > "$WORK/$name.diff"; then
            ok "$name"
        else
            bad "$name (output mismatch)"; sed 's/^/      /' "$WORK/$name.diff"
        fi
    else
        ok "$name (no expected file; ran ok)"
    fi
done

# ------------------------------------------------------------------ stdlib
echo
echo "== stdlib tests (tests/stdlib, compiled with src/stdlib/*.flint) =="
ST="tests/stdlib"
for src in "$ST"/*.flint; do
    name="$(basename "$src" .flint)"
    exp="$ST/expected/$name.out"
    bin="$WORK/st_$name.bin"
    if ! timeout 30 "$FLINTC" src/stdlib/*.flint "$src" -o "$bin" 2> "$WORK/st_$name.cerr"; then
        bad "$name (stdlib compile)"; sed 's/^/      /' "$WORK/st_$name.cerr"; continue
    fi
    timeout 30 "$bin" > "$WORK/st_$name.out" 2>&1
    code=$?
    if [ "$code" -ne 0 ]; then
        bad "$name (exit=$code)"; sed 's/^/      /' "$WORK/st_$name.out"; continue
    fi
    if [ -f "$exp" ]; then
        if diff -u "$exp" "$WORK/st_$name.out" > "$WORK/st_$name.diff"; then
            ok "$name (stdlib)"
        else
            bad "$name (output mismatch)"; sed 's/^/      /' "$WORK/st_$name.diff"
        fi
    else
        ok "$name (no expected file; ran ok)"
    fi
done

# ------------------------------------------------------------------- golden
echo
echo "== golden-asm tests (tests/golden) =="
for gold in tests/golden/*.s; do
    name="$(basename "$gold" .s)"
    if [ -f "tests/cases/$name.flint" ]; then
        src="tests/cases/$name.flint"
    elif [ -f "examples/$name.flint" ]; then
        src="examples/$name.flint"
    else
        bad "$name (no source for golden)"; continue
    fi
    if ! timeout 20 "$FLINTC" "$src" -S > "$WORK/$name.s" 2> "$WORK/$name.serr"; then
        bad "$name (-S failed)"; sed 's/^/      /' "$WORK/$name.serr"; continue
    fi
    if diff -u "$gold" "$WORK/$name.s" > "$WORK/$name.gdiff"; then
        ok "$name"
    else
        bad "$name (asm mismatch)"; sed 's/^/      /' "$WORK/$name.gdiff"
    fi
done

# -------------------------------------------------------------------- errors
echo
echo "== error tests (tests/errors) =="
for src in tests/errors/*.flint; do
    name="$(basename "$src" .flint)"
    exp="tests/errors/expected/$name.err"
    if timeout 20 "$FLINTC" "$src" -o "$WORK/$name.ebin" > "$WORK/$name.out" 2> "$WORK/$name.err"; then
        bad "$name (expected compile failure)"; continue
    fi
    first="$(head -n1 "$exp" 2>/dev/null)"
    if [ -n "$first" ] && grep -qF "$first" "$WORK/$name.err"; then
        ok "$name"
    else
        bad "$name (wrong diagnostic)"; sed 's/^/      /' "$WORK/$name.err"
    fi
done

# ------------------------------------------------------------- multi-file
echo
echo "== multi-file tests (tests/multifile) =="
MF="tests/multifile"
# positional args
if timeout 20 "$FLINTC" "$MF/util.flint" "$MF/main.flint" -o "$WORK/mf_pos.bin" 2> "$WORK/mf_pos.cerr"; then
    timeout 20 "$WORK/mf_pos.bin" > "$WORK/mf_pos.out" 2>&1
    if diff -u "$MF/expected.out" "$WORK/mf_pos.out" > "$WORK/mf_pos.diff"; then
        ok "multifile (positional)"
    else
        bad "multifile (positional)"; sed 's/^/      /' "$WORK/mf_pos.diff"
    fi
else
    bad "multifile (positional, compile)"; sed 's/^/      /' "$WORK/mf_pos.cerr"
fi
# manifest
if timeout 20 "$FLINTC" --manifest "$MF/flint.toml" -o "$WORK/mf_man.bin" 2> "$WORK/mf_man.cerr"; then
    timeout 20 "$WORK/mf_man.bin" > "$WORK/mf_man.out" 2>&1
    if diff -u "$MF/expected.out" "$WORK/mf_man.out" > "$WORK/mf_man.diff"; then
        ok "multifile (manifest)"
    else
        bad "multifile (manifest)"; sed 's/^/      /' "$WORK/mf_man.diff"
    fi
else
    bad "multifile (manifest, compile)"; sed 's/^/      /' "$WORK/mf_man.cerr"
fi
# directory
if timeout 20 "$FLINTC" --dir "$MF" -o "$WORK/mf_dir.bin" 2> "$WORK/mf_dir.cerr"; then
    timeout 20 "$WORK/mf_dir.bin" > "$WORK/mf_dir.out" 2>&1
    if diff -u "$MF/expected.out" "$WORK/mf_dir.out" > "$WORK/mf_dir.diff"; then
        ok "multifile (dir)"
    else
        bad "multifile (dir)"; sed 's/^/      /' "$WORK/mf_dir.diff"
    fi
else
    bad "multifile (dir, compile)"; sed 's/^/      /' "$WORK/mf_dir.cerr"
fi

# ------------------------------------------------------------------ summary
echo
echo "===================================="
echo "PASS=$pass  FAIL=$fail"
[ "$fail" -eq 0 ] && { echo "ALL TESTS PASSED"; exit 0; }
echo "SOME TESTS FAILED"; exit 1
