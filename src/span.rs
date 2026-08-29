/// Source location for error reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn join(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn line_col(&self, src: &str) -> (usize, usize) {
        let start = self.start.min(src.len());
        let up_to = &src[..start];
        let line = up_to.matches('\n').count();
        let line_start = up_to.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = start - line_start;
        (line, col)
    }
}
