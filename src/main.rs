mod ast;
mod backend;
mod error;
mod lexer;
mod middle;
mod parser;
mod span;
mod token;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The freestanding runtime, embedded at compile time.
const INTRINSICS: &str = include_str!("intrinsics.s");

fn print_usage() {
    eprintln!(
        "usage: flintc [OPTIONS] <input.flint>...\n\
         \n\
         Compile one or more Flint files into a freestanding binary. Multiple\n\
         files are concatenated (each keeps its own `package` header).\n\
         \n\
         Options:\n\
         \t-o <FILE>\twrite output to FILE (default: <first input> without .flint)\n\
         \t-S\t\tstop after generating assembly (print to stdout)\n\
         \t--manifest <FILE>\tread source files from a TOML manifest\n\
         \t--dir <DIR>\tcompile every .flint file in DIR (sorted)\n\
         \t-h, --help\tprint this help"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut asm_only = false;
    let mut manifest: Option<String> = None;
    let mut dir: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(o) => output = Some(o.clone()),
                    None => {
                        eprintln!("error: -o requires an argument");
                        return ExitCode::from(2);
                    }
                }
            }
            "-S" => asm_only = true,
            "--manifest" => {
                i += 1;
                match args.get(i) {
                    Some(m) => manifest = Some(m.clone()),
                    None => {
                        eprintln!("error: --manifest requires an argument");
                        return ExitCode::from(2);
                    }
                }
            }
            "--dir" => {
                i += 1;
                match args.get(i) {
                    Some(d) => dir = Some(d.clone()),
                    None => {
                        eprintln!("error: --dir requires an argument");
                        return ExitCode::from(2);
                    }
                }
            }
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            a if a.starts_with('-') && a != "-" => {
                eprintln!("error: unknown option '{}'", a);
                return ExitCode::from(2);
            }
            a => inputs.push(a.to_string()),
        }
        i += 1;
    }

    // Determine the list of source files.
    let files: Vec<PathBuf> = match (manifest, dir, inputs.is_empty()) {
        (Some(m), _, _) => match read_manifest(&m) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(1);
            }
        },
        (_, Some(d), _) => match read_dir_flint(&d) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(1);
            }
        },
        (_, _, true) => {
            eprintln!("error: no input file");
            print_usage();
            return ExitCode::from(2);
        }
        (_, _, false) => inputs.iter().map(PathBuf::from).collect(),
    };

    // Read each source file (each keeps its own `package` header).
    let mut file_sources: Vec<String> = Vec::new();
    for f in &files {
        match fs::read_to_string(f) {
            Ok(s) => file_sources.push(s),
            Err(e) => {
                eprintln!("error: cannot read '{}': {}", f.display(), e);
                return ExitCode::from(1);
            }
        }
    }
    // The concatenated source is used for error reporting (global spans).
    let src = file_sources.join("\n");
    // The default output is derived from the first source file.
    let first = files
        .first()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "out".to_string());

    let asm = match compile_sources(&file_sources) {
        Ok(a) => a,
        Err(e) => {
            error::die(&src, &e);
        }
    };

    if asm_only {
        if let Some(o) = &output {
            if let Err(e) = fs::write(o, &asm) {
                eprintln!("error: write {}: {}", o.as_str(), e);
                return ExitCode::from(1);
            }
        } else {
            print!("{}", asm);
        }
        return ExitCode::SUCCESS;
    }

    let out_path = match output {
        Some(o) => PathBuf::from(o),
        None => default_output(&first),
    };

    match build(&src, &asm, &out_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {}", msg);
            ExitCode::from(1)
        }
    }
}

/// Run the full pipeline over multiple input files. The concatenated source is
/// lexed once (so error spans are global); a single global name registry is
/// pre-scanned with the running package reset at each file boundary (so a
/// packageless file does not inherit the previous file's package); each file
/// is then parsed against that registry with its own package state; the
/// programs are merged and monomorphized; and the assembly is generated.
pub fn compile_sources(file_sources: &[String]) -> error::CompileResult<String> {
    let src = file_sources.join("\n");
    let toks = lexer::tokenize(&src)?;
    // Per-file token ranges. Each file is lexed individually for its count
    // (minus its trailing Eof); the counts partition `toks` because the
    // concatenated lex has no token crossing a file boundary.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut off = 0usize;
    for fs in file_sources {
        let n = lexer::tokenize(fs)?.len() - 1; // drop the appended Eof
        ranges.push((off, off + n));
        off += n;
    }
    debug_assert_eq!(off, toks.len() - 1, "per-file token counts must partition the stream");
    // One global name registry, resetting the package at each file start.
    let boundaries: std::collections::HashSet<usize> =
        ranges.iter().map(|(s, _)| *s).collect();
    let reg = parser::prescan(&toks, &boundaries);
    // The parser stops at an Eof, so each file slice needs its own Eof.
    let eof = toks[toks.len() - 1].clone();
    // Parse each file slice with the global registry and fresh package state,
    // then merge the programs.
    let mut merged = ast::Program {
        structs: Vec::new(),
        funcs: Vec::new(),
        interfaces: Vec::new(),
        enums: Vec::new(),
        imports: Vec::new(),
    };
    for (start, end) in &ranges {
        let mut file_toks = toks[*start..*end].to_vec();
        file_toks.push(eof.clone());
        let mut p = parser::Parser::with_registry(&file_toks, &reg);
        let mut prog = p.parse_program()?;
        merged.structs.append(&mut prog.structs);
        merged.funcs.append(&mut prog.funcs);
        merged.interfaces.append(&mut prog.interfaces);
        merged.enums.append(&mut prog.enums);
        merged.imports.append(&mut prog.imports);
    }
    let prog = middle::monomorphize(&merged)?;
    backend::generate(&prog)
}

fn default_output(input: &str) -> PathBuf {
    let p = Path::new(input);
    let file_name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "out".to_string());
    let stem = file_name
        .strip_suffix(".flint")
        .map(|s| s.to_string())
        .unwrap_or(file_name.clone());
    let parent = p.parent().unwrap_or(Path::new("."));
    parent.join(stem)
}

/// Read a TOML manifest and return the source files (relative to the manifest).
/// The manifest must contain a `sources = ["a.flint", "b.flint", ...]` array.
fn read_manifest(path: &str) -> Result<Vec<PathBuf>, String> {
    let p = Path::new(path);
    let content = fs::read_to_string(p)
        .map_err(|e| format!("cannot read manifest '{}': {}", p.display(), e))?;
    let dir = p.parent().unwrap_or(Path::new("."));
    let mut sources = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("sources") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();
                if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    for item in inner.split(',') {
                        let item = item.trim().trim_matches('"').trim_matches('\'');
                        if !item.is_empty() {
                            sources.push(dir.join(item));
                        }
                    }
                }
            }
        }
    }
    if sources.is_empty() {
        return Err(format!("no 'sources' array found in manifest '{}'", p.display()));
    }
    Ok(sources)
}

/// Read every `.flint` file in a directory (sorted by name).
fn read_dir_flint(dir: &str) -> Result<Vec<PathBuf>, String> {
    let p = Path::new(dir);
    let mut files: Vec<PathBuf> = fs::read_dir(p)
        .map_err(|e| format!("cannot read directory '{}': {}", p.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|path| path.is_file() && path.extension().map(|s| s == "flint").unwrap_or(false))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no .flint files found in directory '{}'", p.display()));
    }
    Ok(files)
}

fn run_tool(name: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(name)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run '{}': {}", name, e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "'{} {}' failed ({}):\n{}\n{}",
            name,
            args.join(" "),
            out.status,
            stderr,
            stdout
        ));
    }
    Ok(())
}

fn build(_src: &str, asm: &str, out_path: &Path) -> Result<(), String> {
    let dir = env::temp_dir().join(format!("flintc-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create temp dir: {}", e))?;

    let user_s = dir.join("user.s");
    let intr_s = dir.join("intr.s");
    let user_o = dir.join("user.o");
    let intr_o = dir.join("intr.o");

    fs::write(&user_s, asm).map_err(|e| format!("write {}: {}", user_s.display(), e))?;
    fs::write(&intr_s, INTRINSICS).map_err(|e| format!("write {}: {}", intr_s.display(), e))?;

    run_tool("as", &[&user_s.to_string_lossy(), "-o", &user_o.to_string_lossy()])?;
    run_tool("as", &[&intr_s.to_string_lossy(), "-o", &intr_o.to_string_lossy()])?;

    // Ensure the output directory exists.
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {}", parent.display(), e))?;
        }
    }

    run_tool(
        "ld",
        &[
            "-e",
            "_start",
            "-o",
            &out_path.to_string_lossy(),
            &user_o.to_string_lossy(),
            &intr_o.to_string_lossy(),
        ],
    )?;

    Ok(())
}
