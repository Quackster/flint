// Integration tests for the flintc driver: full pipeline (compile -> as -> ld -> run),
// golden-assembly checks, and spanned-error checks. These run the compiled `flintc`
// binary end-to-end and are independent of tests/run_tests.sh.

use std::process::Command;

fn flintc() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_flintc"))
}

fn read(p: &str) -> String {
    std::fs::read_to_string(p).expect("read fixture")
}

static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Compile `src` to a uniquely-named binary in a temp dir, run it, return
/// (stdout, exit_code). Unique name so parallel tests don't collide.
fn compile_and_run(src: &str) -> (String, i32) {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let bin = std::env::temp_dir().join(format!(
        "flintc_{}_{}_bin",
        std::process::id(),
        n
    ));
    let out = Command::new(flintc())
        .arg(src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run flintc");
    assert!(
        out.status.success(),
        "compile {} failed: {}",
        src,
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&bin).output().expect("run binary");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    let _ = std::fs::remove_file(&bin);
    (stdout, run.status.code().unwrap_or(-1))
}

/// Generate assembly only (flintc -S) and return stdout.
fn asm_of(src: &str) -> String {
    let out = Command::new(flintc())
        .arg(src)
        .arg("-S")
        .output()
        .expect("run flintc -S");
    assert!(out.status.success(), "flintc -S {} failed", src);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn hello_runs() {
    let (out, code) = compile_and_run("tests/cases/hello.flint");
    assert_eq!(code, 0);
    assert_eq!(out, "hello, world\n");
}

#[test]
fn fib_runs() {
    let (out, code) = compile_and_run("examples/fib.flint");
    assert_eq!(code, 0);
    assert_eq!(out, "55\n");
}

#[test]
fn arithmetic_results() {
    let (out, code) = compile_and_run("tests/cases/arithmetic.flint");
    assert_eq!(code, 0);
    assert_eq!(out, "14\n20\n3\n2\n8\n4\n-8\n");
}

#[test]
fn structs_and_pointers() {
    let (out, code) = compile_and_run("tests/cases/structs.flint");
    assert_eq!(code, 0);
    assert_eq!(out, "3,4\n14\n1,2,70\n50\n");
}

#[test]
fn golden_hello_asm() {
    let asm = asm_of("tests/cases/hello.flint");
    let golden = read("tests/golden/hello.s");
    assert_eq!(asm, golden, "hello.assembly diverged from golden");
}

#[test]
fn asm_has_expected_intrinsic_calls() {
    let asm = asm_of("tests/cases/hello.flint");
    assert!(asm.contains("call flint_printstr"));
    assert!(asm.contains("_start:"));
    assert!(asm.contains("call flint_exit"));
    assert!(asm.contains(".asciz \"hello, world\""));
    assert!(asm.contains(".asciz \"\\n\""));
}

#[test]
fn spanned_parse_error() {
    let out = Command::new(flintc())
        .arg("tests/errors/badexpr.flint")
        .arg("-o")
        .arg(std::env::temp_dir().join("flintc_err_bin"))
        .output()
        .expect("run flintc");
    assert!(!out.status.success(), "badexpr.flint should not compile");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("error[2:16]"), "missing span: {}", err);
    assert!(err.contains("expected expression"), "missing message: {}", err);
}

#[test]
fn undefined_variable_error() {
    let out = Command::new(flintc())
        .arg("tests/errors/undef.flint")
        .arg("-o")
        .arg(std::env::temp_dir().join("flintc_err_bin"))
        .output()
        .expect("run flintc");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("undefined variable"), "got: {}", err);
}
