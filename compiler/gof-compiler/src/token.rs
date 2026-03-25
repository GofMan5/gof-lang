use crate::source::Span;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TokenKind {
    Module,
    Fn,
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
    Ident(String),
    IntLiteral(i64),
    StringLiteral(String),
    LParen,
    RParen,
    Colon,
    Comma,
    Equal,
    Plus,
    Minus,
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
