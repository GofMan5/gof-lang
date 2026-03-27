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
    For,
    In,
    Break,
    Continue,
    Go,
    Return,
    Struct,
    Enum,
    Protocol,
    Match,
    Async,
    Await,
    Select,
    Default,
    Defer,
    Unsafe,
    Mut,
    And,
    Or,
    Not,
    True,
    False,
    Ident(String),
    IntLiteral(i64),
    StringLiteral(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Dot,
    Colon,
    Comma,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Question,
    Arrow,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
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
