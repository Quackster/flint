use crate::span::Span;
use std::process;

/// A compiler error tied to a source location.
#[derive(Debug)]
pub struct CompileError {
    pub span: Span,
    pub message: String,
}

impl CompileError {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

pub type CompileResult<T> = Result<T, CompileError>;

/// Render an error with a caret pointing at the offending source line.
pub fn render(src: &str, err: &CompileError) -> String {
    let (line, col) = err.span.line_col(src);
    let line_text = src.lines().nth(line).unwrap_or("");
    let underline = " ".repeat(col);
    format!(
        "error[{}:{}]: {}\n  | {}\n  | {}",
        line + 1,
        col + 1,
        err.message,
        line_text,
        format!("{}^", underline)
    )
}

/// Print the error and set the process exit code to 1.
pub fn die(src: &str, err: &CompileError) -> ! {
    eprintln!("{}", render(src, err));
    process::exit(1);
}
