use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use crate::token::{Token, TokenKind};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum Stmt {
    Return(Expr, Span),
    Bind {
        name: String,
        mutable: bool,
        value: Expr,
        span: Span,
    },
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    Expr(Expr, Span),
}

#[derive(Debug, Clone, Serialize)]
pub enum Expr {
    Int(i64, Span),
    String(String, Span),
    Ident(String, Span),
    Call {
        callee: String,
        args: Vec<Expr>,
        span: Span,
    },
    Binary {
        lhs: Box<Expr>,
        op: BinaryOp,
        rhs: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum BinaryOp {
    Add,
    Sub,
}

pub fn parse(cst: &CstModule) -> Result<Module, Diagnostics> {
    let mut parser = Parser::new(&cst.tokens);
    let module = parser.parse_module();
    if parser.diagnostics.is_empty() {
        Ok(module)
    } else {
        Err(parser.diagnostics)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
    diagnostics: Diagnostics,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            current: 0,
            diagnostics: Diagnostics::default(),
        }
    }

    fn parse_module(&mut self) -> Module {
        let mut functions = Vec::new();
        while !self.at_end() {
            self.skip_newlines();
            if self.matches(TokenDiscriminant::Eof) {
                break;
            }
            functions.push(self.parse_function());
        }
        Module { functions }
    }

    fn parse_function(&mut self) -> Function {
        let start = self.expect(TokenDiscriminant::Fn, "expected `fn` to start a function");
        let name = self.expect_ident("expected a function name after `fn`");
        self.expect(
            TokenDiscriminant::LParen,
            "expected `(` after function name",
        );
        let params = self.parse_params();
        self.expect(
            TokenDiscriminant::RParen,
            "expected `)` after function parameters",
        );
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after function signature",
        );
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after function signature",
        );
        self.expect(
            TokenDiscriminant::Indent,
            "expected an indented block after function signature",
        );

        let mut body = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            body.push(self.parse_stmt());
        }

        self.expect(
            TokenDiscriminant::Dedent,
            "expected block dedent after function body",
        );

        Function {
            name,
            params,
            body,
            span: Span::new(start.line, start.column, start.end_column),
        }
    }

    fn parse_params(&mut self) -> Vec<String> {
        let mut params = Vec::new();
        if self.check(TokenDiscriminant::RParen) {
            return params;
        }

        loop {
            params.push(self.expect_ident("expected parameter name"));
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        params
    }

    fn parse_stmt(&mut self) -> Stmt {
        if self.matches(TokenDiscriminant::Return) {
            let span = self.previous().span;
            let expr = self.parse_expr();
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after `return`",
            );
            return Stmt::Return(expr, span);
        }

        if self.matches(TokenDiscriminant::Mut) {
            let span = self.previous().span;
            let name = self.expect_ident("expected an identifier after `mut`");
            self.expect(TokenDiscriminant::Equal, "expected `=` in a binding");
            let value = self.parse_expr();
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after binding",
            );
            return Stmt::Bind {
                name,
                mutable: true,
                value,
                span,
            };
        }

        if self.check_ident_assignment() {
            let name = self.expect_ident("expected a binding target");
            let span = self.previous().span;
            self.expect(
                TokenDiscriminant::Equal,
                "expected `=` in an assignment or binding",
            );
            let value = self.parse_expr();
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after assignment",
            );
            return Stmt::Assign { name, value, span };
        }

        let expr = self.parse_expr();
        let span = expr.span();
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after expression",
        );
        Stmt::Expr(expr, span)
    }

    fn parse_expr(&mut self) -> Expr {
        let mut expr = self.parse_call();

        while self.matches(TokenDiscriminant::Plus) || self.matches(TokenDiscriminant::Minus) {
            let operator = if self.previous_kind_matches(TokenDiscriminant::Plus) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let rhs = self.parse_call();
            let span = Span::new(expr.span().line, expr.span().column, rhs.span().end_column);
            expr = Expr::Binary {
                lhs: Box::new(expr),
                op: operator,
                rhs: Box::new(rhs),
                span,
            };
        }

        expr
    }

    fn parse_call(&mut self) -> Expr {
        let primary = self.parse_primary();
        if !self.matches(TokenDiscriminant::LParen) {
            return primary;
        }

        let callee = match primary {
            Expr::Ident(name, span) => (name, span),
            other => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2001",
                        "only named functions can be called in the bootstrap parser",
                        "call expressions currently require an identifier callee",
                        other.span(),
                    )
                    .with_fix_it("replace the callee with a function name"),
                );
                ("_error".to_string(), other.span())
            }
        };

        let args = self.parse_args();
        let end = self.expect(
            TokenDiscriminant::RParen,
            "expected `)` after call arguments",
        );
        Expr::Call {
            callee: callee.0,
            args,
            span: Span::new(callee.1.line, callee.1.column, end.end_column),
        }
    }

    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if self.check(TokenDiscriminant::RParen) {
            return args;
        }

        loop {
            args.push(self.parse_expr());
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        args
    }

    fn parse_primary(&mut self) -> Expr {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::IntLiteral(value) => Expr::Int(value, token.span),
            TokenKind::StringLiteral(value) => Expr::String(value, token.span),
            TokenKind::Ident(value) => Expr::Ident(value, token.span),
            TokenKind::LParen => {
                let expr = self.parse_expr();
                self.expect(
                    TokenDiscriminant::RParen,
                    "expected `)` to close grouped expression",
                );
                expr
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2001",
                        "unexpected token in expression",
                        "expected an identifier, literal, or grouped expression",
                        token.span,
                    )
                    .with_fix_it("replace this token with a valid expression"),
                );
                Expr::Ident("_error".to_string(), token.span)
            }
        }
    }

    fn expect(&mut self, expected: TokenDiscriminant, message: &'static str) -> Span {
        if self.check(expected) {
            return self.advance().span;
        }

        let token = self.peek();
        let token_span = token.span;
        let token_name = token_debug_name(&token.kind);
        self.diagnostics.push(
            Diagnostic::error(
                "GOF2002",
                message,
                format!("found `{token_name}` instead"),
                token_span,
            )
            .with_fix_it(format!("insert or replace with {}", expected.as_hint())),
        );
        token_span
    }

    fn expect_ident(&mut self, message: &'static str) -> String {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Ident(value) => value,
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2002",
                        message,
                        "identifiers must start with a letter or underscore",
                        token.span,
                    )
                    .with_fix_it("replace this token with an identifier"),
                );
                "_error".to_string()
            }
        }
    }

    fn skip_newlines(&mut self) {
        while self.matches(TokenDiscriminant::Newline) {}
    }

    fn matches(&mut self, expected: TokenDiscriminant) -> bool {
        if self.check(expected) {
            self.advance();
            return true;
        }
        false
    }

    fn check(&self, expected: TokenDiscriminant) -> bool {
        !self.at_end() && expected.matches(&self.peek().kind)
    }

    fn check_ident_assignment(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(
                self.peek_next().map(|token| &token.kind),
                Some(TokenKind::Equal)
            )
    }

    fn previous_kind_matches(&self, expected: TokenDiscriminant) -> bool {
        expected.matches(&self.previous().kind)
    }

    fn advance(&mut self) -> &Token {
        if !self.at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn peek_next(&self) -> Option<&Token> {
        self.tokens.get(self.current + 1)
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, span)
            | Expr::String(_, span)
            | Expr::Ident(_, span)
            | Expr::Call { span, .. }
            | Expr::Binary { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TokenDiscriminant {
    Fn,
    Return,
    Mut,
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

impl TokenDiscriminant {
    fn matches(self, kind: &TokenKind) -> bool {
        matches!(
            (self, kind),
            (Self::Fn, TokenKind::Fn)
                | (Self::Return, TokenKind::Return)
                | (Self::Mut, TokenKind::Mut)
                | (Self::LParen, TokenKind::LParen)
                | (Self::RParen, TokenKind::RParen)
                | (Self::Colon, TokenKind::Colon)
                | (Self::Comma, TokenKind::Comma)
                | (Self::Equal, TokenKind::Equal)
                | (Self::Plus, TokenKind::Plus)
                | (Self::Minus, TokenKind::Minus)
                | (Self::Newline, TokenKind::Newline)
                | (Self::Indent, TokenKind::Indent)
                | (Self::Dedent, TokenKind::Dedent)
                | (Self::Eof, TokenKind::Eof)
        )
    }

    fn as_hint(self) -> &'static str {
        match self {
            Self::Fn => "`fn`",
            Self::Return => "`return`",
            Self::Mut => "`mut`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::Colon => "`:`",
            Self::Comma => "`,`",
            Self::Equal => "`=`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Newline => "a newline",
            Self::Indent => "an indented block",
            Self::Dedent => "a dedent",
            Self::Eof => "end of file",
        }
    }
}

fn token_debug_name(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(value) => format!("identifier `{value}`"),
        TokenKind::IntLiteral(value) => format!("int literal `{value}`"),
        TokenKind::StringLiteral(value) => format!("string literal `{value}`"),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Expr, Stmt, parse};
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;

    #[test]
    fn parses_function_module() {
        let source = SourceFile::new("test.gof", "fn main():\n    return 40 + 2\n");
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }

    #[test]
    fn parses_bindings_and_calls() {
        let source = SourceFile::new(
            "test.gof",
            "fn add(a, b):\n    return a + b\nfn main():\n    mut total = add(40, 1)\n    total = total + 1\n    return total\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.functions.len(), 2);
        assert!(matches!(
            &module.functions[1].body[0],
            Stmt::Bind { mutable: true, .. }
        ));
        assert!(matches!(
            &module.functions[1].body[2],
            Stmt::Return(Expr::Ident(name, _), _) if name == "total"
        ));
    }
}
