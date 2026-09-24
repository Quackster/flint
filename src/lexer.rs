use crate::error::{CompileError, CompileResult};
use crate::span::Span;
use crate::token::{Token, Tok};

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.pos)
    }

    fn skip_ws(&mut self) -> CompileResult<()> {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                    self.bump();
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    self.bump();
                    self.bump();
                    while !self.at_eof() && self.peek() != Some(b'\n') {
                        self.bump();
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let start = self.pos;
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while !self.at_eof() {
                        if self.peek() == Some(b'*') && self.peek_at(1) == Some(b'/') {
                            self.bump();
                            self.bump();
                            closed = true;
                            break;
                        }
                        self.bump();
                    }
                    if !closed {
                        return Err(CompileError::new(
                            self.span_from(start),
                            "unterminated block comment",
                        ));
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn read_number(&mut self, start: usize) -> CompileResult<Tok> {
        let is_hex = self.peek() == Some(b'0') && self.peek_at(1) == Some(b'x');
        if is_hex {
            self.pos += 2;
        }
        let num_start = self.pos;
        while let Some(b) = self.peek() {
            if is_hex {
                if !b.is_ascii_hexdigit() {
                    break;
                }
            } else if !b.is_ascii_digit() {
                break;
            }
            self.pos += 1;
        }
        let text = &self.src[num_start..self.pos];
        let span = self.span_from(start);
        // Parse as u64 and wrap to i64 (two's complement), so literals like
        // 0x8000000000000000 or 0xFFFFFFFFFFFFFFFF are representable.
        let val = if is_hex {
            match u64::from_str_radix(text, 16) {
                Ok(v) => v as i64,
                Err(_) => {
                    return Err(CompileError::new(span, format!("invalid hex literal '0x{}'", text)))
                }
            }
        } else {
            match text.parse::<i64>().or_else(|_| text.parse::<u64>().map(|v| v as i64)) {
                Ok(v) => v,
                Err(_) => {
                    return Err(CompileError::new(span, format!("invalid integer literal '{}'", text)))
                }
            }
        };
        Ok(Tok::Int(val))
    }

    fn read_ident(&mut self, start: usize) -> Tok {
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.src[start..self.pos];
        match text {
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "for" => Tok::For,
            "return" => Tok::Return,
            "class" => Tok::Class,
            "new" => Tok::New,
            "void" => Tok::Void,
            "true" => Tok::True,
            "false" => Tok::False,
            "private" => Tok::Private,
            "public" => Tok::Public,
            "static" => Tok::Static,
            "this" => Tok::This,
            "get" => Tok::Get,
            "set" => Tok::Set,
            "getset" => Tok::GetSet,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "switch" => Tok::Switch,
            "case" => Tok::Case,
            "default" => Tok::Default,
            "null" => Tok::Null,
            "interface" => Tok::Interface,
            "abstract" => Tok::Abstract,
            "extends" => Tok::Extends,
            "implements" => Tok::Implements,
            "super" => Tok::Super,
            "instanceof" => Tok::Instanceof,
            "package" => Tok::Package,
            "import" => Tok::Import,
            "enum" => Tok::Enum,
            "throw" => Tok::Throw,
            "try" => Tok::Try,
            "catch" => Tok::Catch,
            "finally" => Tok::Finally,
            "async" => Tok::Async,
            "byte" => Tok::KwByte,
            "short" => Tok::KwShort,
            "int" => Tok::KwInt,
            "long" => Tok::KwLong,
            "char" => Tok::KwChar,
            "boolean" => Tok::KwBoolean,
            "string" => Tok::KwString,
            "list" => Tok::KwList,
            "queue" => Tok::KwQueue,
            "hashmap" => Tok::KwHashMap,
            "hashset" => Tok::KwHashSet,
            "dictionary" => Tok::KwDict,
            _ => Tok::Ident(text.to_string()),
        }
    }

    fn read_string(&mut self, start: usize) -> CompileResult<Token> {
        // self.pos is at the opening quote.
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(CompileError::new(
                        self.span_from(start),
                        "unterminated string literal",
                    ));
                }
                Some(b'"') => break,
                Some(b'\\') => match self.bump() {
                    Some(b'n') => out.push('\n'),
                    Some(b't') => out.push('\t'),
                    Some(b'r') => out.push('\r'),
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'0') => out.push('\0'),
                    Some(c) => {
                        out.push('\\');
                        out.push(c as char);
                    }
                    None => {
                        return Err(CompileError::new(
                            self.span_from(start),
                            "unterminated string escape",
                        ));
                    }
                },
                Some(c) => out.push(c as char),
            }
        }
        Ok(Token {
            kind: Tok::Str(out),
            span: self.span_from(start),
        })
    }

    pub fn tokenize(&mut self) -> CompileResult<Vec<Token>> {
        let mut toks = Vec::new();
        loop {
            self.skip_ws()?;
            if self.at_eof() {
                let span = self.span_from(self.bytes.len());
                toks.push(Token {
                    kind: Tok::Eof,
                    span,
                });
                return Ok(toks);
            }
            let start = self.pos;
            let b = self.bump().unwrap();
            let tok = match b {
                b'0'..=b'9' => {
                    self.pos = start;
                    Token {
                        kind: self.read_number(start)?,
                        span: self.span_from(start),
                    }
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    self.pos = start;
                    Token {
                        kind: self.read_ident(start),
                        span: self.span_from(start),
                    }
                }
                b'"' => {
                    self.pos = start;
                    self.read_string(start)?
                }
                _ => {
                    let kind = self
                        .punct(b)
                        .ok_or_else(|| CompileError::new(
                            self.span_from(start),
                            format!("unexpected character '{}'", b as char),
                        ))?;
                    Token {
                        kind,
                        span: self.span_from(start),
                    }
                }
            };
            toks.push(tok);
        }
    }

    /// Consume any multi-char operator continuation for a single-char start.
    fn punct(&mut self, b: u8) -> Option<Tok> {
        let t = match b {
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b'{' => Tok::LBrace,
            b'}' => Tok::RBrace,
            b'[' => Tok::LBracket,
            b']' => Tok::RBracket,
            b',' => Tok::Comma,
            b';' => Tok::Semicolon,
            b'.' => Tok::Dot,
            b':' => Tok::Colon,
            b'+' => {
                if self.peek() == Some(b'+') {
                    self.bump();
                    Tok::Incr
                } else if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::PlusEq
                } else {
                    Tok::Plus
                }
            }
            b'-' => {
                if self.peek() == Some(b'-') {
                    self.bump();
                    Tok::Decr
                } else if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::MinusEq
                } else {
                    Tok::Minus
                }
            }
            b'*' => {
                if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::StarEq
                } else {
                    Tok::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::SlashEq
                } else {
                    Tok::Slash
                }
            }
            b'?' => Tok::Question,
            b'%' => Tok::Percent,
            b'&' => {
                if self.peek() == Some(b'&') {
                    self.bump();
                    Tok::AndAnd
                } else {
                    Tok::Amp
                }
            }
            b'|' => {
                if self.peek() == Some(b'|') {
                    self.bump();
                    Tok::OrOr
                } else {
                    Tok::Pipe
                }
            }
            b'^' => Tok::Caret,
            b'@' => Tok::At,
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::BangEq
                } else {
                    Tok::Bang
                }
            }
            b'<' => match self.peek() {
                Some(b'=') => {
                    self.bump();
                    Tok::LtEq
                }
                Some(b'<') => {
                    self.bump();
                    Tok::Shl
                }
                _ => Tok::Lt,
            },
            b'>' => match self.peek() {
                Some(b'=') => {
                    self.bump();
                    Tok::GtEq
                }
                Some(b'>') => {
                    self.bump();
                    Tok::Shr
                }
                _ => Tok::Gt,
            },
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.bump();
                    Tok::EqEq
                } else {
                    Tok::Assign
                }
            }
            other => {
                let _ = other;
                return None;
            }
        };
        Some(t)
    }
}

pub fn tokenize(src: &str) -> CompileResult<Vec<Token>> {
    let mut lx = Lexer::new(src);
    lx.tokenize()
}
