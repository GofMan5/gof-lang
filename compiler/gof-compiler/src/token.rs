use crate::source::Span;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TokenKind {
    Module,
    Import,
    Fn,
    If,
    Else,
    While,
    Go,
    Return,
    Struct,
    Enum,
    Protocol,
    Match,
    Async,
    Await,
    Select,
    Defer,
    Unsafe,
    Mut,
    True,
    False,
    Ident(String),
    IntLiteral(i64),
    StringLiteral(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Arrow,
    Plus,
    Minus,
    Star,
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
