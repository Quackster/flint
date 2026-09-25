use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    // literals
    Int(i64),
    Str(String),
    Ident(String),

    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Dot,
    Colon,
    Question,

    // operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    AndAnd,
    OrOr,
    At,       // @ (function address)
    Shl,
    Shr,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Assign,
    Bang,
    Incr, // ++
    Decr, // --
    PlusEq,   // +=
    MinusEq,  // -=
    StarEq,   // *=
    SlashEq,  // /=

    // keywords
    If,
    Else,
    While,
    For,
    Return,
    Class,
    New,
    Void,
    True,
    False,
    Private,
    Public,
    Static,
    This,
    Get,
    Set,
    GetSet,
    Break,
    Continue,
    Switch,
    Case,
    Default,
    Null,
    Interface,
    Abstract,
    Extends,
    Implements,
    Super,
    Instanceof,
    Package,
    Import,
    Enum,
    Throw,
    Try,
    Catch,
    Finally,
    Async,
    Await,
    // type keywords
    KwByte,
    KwShort,
    KwInt,
    KwLong,
    KwChar,
    KwBoolean,
    KwString,

    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    pub span: Span,
}
