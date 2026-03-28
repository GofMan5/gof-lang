use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use crate::token::{Token, TokenKind};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub imports: Vec<Import>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub module: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub kind: FunctionKind,
    pub fixture_scope: Option<FixtureScopeRef>,
    pub receiver_type: Option<TypeRef>,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: Vec<Stmt>,
    pub span: Span,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FunctionKind {
    Function,
    Test,
    Fixture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FixtureScope {
    Test,
    Module,
}

impl FixtureScope {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Module => "module",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixtureScopeRef {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
    pub span: Span,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructField {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<EnumVariantField>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnumVariantField {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum Stmt {
    Return(Expr, Span),
    Bind {
        name: String,
        mutable: bool,
        ty: Option<TypeRef>,
        value: Expr,
        span: Span,
    },
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    For {
        binding: String,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Match {
        value: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Select {
        arms: Vec<SelectArm>,
        span: Span,
    },
    Expr(Expr, Span),
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum MatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<String>,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectArm {
    pub binding: Option<String>,
    pub kind: SelectArmKind,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum SelectArmKind {
    Operation { operation: Expr },
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictEntry {
    pub key: Expr,
    pub value: Expr,
}

#[derive(Debug, Clone, Serialize)]
pub enum Expr {
    Int(i64, Span),
    String(String, Span),
    Bool(bool, Span),
    Ident(String, Span),
    List {
        items: Vec<Expr>,
        span: Span,
    },
    Dict {
        entries: Vec<DictEntry>,
        span: Span,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
        span: Span,
    },
    Field {
        target: Box<Expr>,
        field: String,
        span: Span,
    },
    MethodCall {
        target: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        span: Span,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Go {
        value: Box<Expr>,
        span: Span,
    },
    Await {
        value: Box<Expr>,
        span: Span,
    },
    Propagate {
        value: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        value: Box<Expr>,
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
    Mul,
    Div,
    Mod,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum UnaryOp {
    Not,
    Neg,
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
        let mut imports = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        while !self.at_end() {
            self.skip_newlines();
            if self.matches(TokenDiscriminant::Eof) {
                break;
            }
            if self.check(TokenDiscriminant::Import) {
                imports.push(self.parse_import());
            } else if self.check(TokenDiscriminant::Struct) {
                structs.push(self.parse_struct());
            } else if self.check(TokenDiscriminant::Enum) {
                enums.push(self.parse_enum());
            } else {
                functions.push(self.parse_function());
            }
        }
        Module {
            imports,
            structs,
            enums,
            functions,
        }
    }

    fn parse_import(&mut self) -> Import {
        let span = self.expect(TokenDiscriminant::Import, "expected `import`");
        let module = self.expect_ident("expected a module name after `import`");
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after import statement",
        );
        Import { module, span }
    }

    fn parse_function(&mut self) -> Function {
        let (kind, fixture_scope, start) = if self.matches(TokenDiscriminant::Test) {
            let start = self.previous().span;
            self.expect(TokenDiscriminant::Fn, "expected `fn` after `test`");
            (FunctionKind::Test, None, start)
        } else if self.matches(TokenDiscriminant::Fixture) {
            let start = self.previous().span;
            self.expect(
                TokenDiscriminant::LParen,
                "expected `(` after `fixture` to declare a lifetime scope",
            );
            let (scope, scope_span) = self.parse_fixture_scope_name();
            self.expect(
                TokenDiscriminant::RParen,
                "expected `)` after the fixture scope",
            );
            self.expect(TokenDiscriminant::Fn, "expected `fn` after `fixture(scope)`");
            (
                FunctionKind::Fixture,
                Some(FixtureScopeRef {
                    name: scope,
                    span: scope_span,
                }),
                start,
            )
        } else {
            (
                FunctionKind::Function,
                None,
                self.expect(TokenDiscriminant::Fn, "expected `fn` to start a function"),
            )
        };
        let head = self.expect_ident("expected a function name after `fn`");
        let head_span = self.previous().span;
        let (receiver_type, name) = if self.matches(TokenDiscriminant::Dot) {
            (
                Some(TypeRef {
                    name: head,
                    args: Vec::new(),
                    span: head_span,
                }),
                self.expect_member_name("expected a method name after `TypeName.`"),
            )
        } else {
            (None, head)
        };
        self.expect(
            TokenDiscriminant::LParen,
            "expected `(` after function name",
        );
        let params = self.parse_params();
        self.expect(
            TokenDiscriminant::RParen,
            "expected `)` after function parameters",
        );
        let return_type = self.parse_optional_return_type_ref();
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after function signature",
        );
        let body = self.parse_block("expected an indented block after function signature");

        Function {
            kind,
            fixture_scope,
            receiver_type,
            name,
            params,
            return_type,
            body,
            span: Span::new(start.line, start.column, start.end_column),
            source_path: PathBuf::new(),
        }
    }

    fn parse_fixture_scope_name(&mut self) -> (String, Span) {
        if self.matches(TokenDiscriminant::Test) {
            return ("test".to_string(), self.previous().span);
        }
        if self.matches(TokenDiscriminant::Module) {
            return ("module".to_string(), self.previous().span);
        }
        let scope = self.expect_ident("expected a fixture scope like `test` or `module`");
        (scope, self.previous().span)
    }

    fn parse_struct(&mut self) -> StructDecl {
        let start = self.expect(
            TokenDiscriminant::Struct,
            "expected `struct` to start a struct",
        );
        let name = self.expect_ident("expected a struct name after `struct`");
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after struct declaration",
        );
        let fields =
            self.parse_struct_fields("expected an indented block after struct declaration");

        StructDecl {
            name,
            fields,
            span: Span::new(start.line, start.column, start.end_column),
            source_path: PathBuf::new(),
        }
    }

    fn parse_enum(&mut self) -> EnumDecl {
        let start = self.expect(TokenDiscriminant::Enum, "expected `enum` to start an enum");
        let name = self.expect_ident("expected an enum name after `enum`");
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after enum declaration",
        );
        let variants =
            self.parse_enum_variants("expected an indented block after enum declaration");

        EnumDecl {
            name,
            variants,
            span: Span::new(start.line, start.column, start.end_column),
            source_path: PathBuf::new(),
        }
    }

    fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if self.check(TokenDiscriminant::RParen) {
            return params;
        }

        loop {
            params.push(self.parse_param());
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        params
    }

    fn parse_param(&mut self) -> Param {
        let name = self.expect_ident("expected parameter name");
        let span = self.previous().span;
        let ty = self.parse_optional_type_ref();
        Param { name, ty, span }
    }

    fn parse_struct_fields(&mut self, message: &'static str) -> Vec<StructField> {
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline before a struct body",
        );
        self.expect(TokenDiscriminant::Indent, message);

        let mut fields = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            fields.push(self.parse_struct_field());
        }

        self.expect(TokenDiscriminant::Dedent, "expected a struct body dedent");
        fields
    }

    fn parse_struct_field(&mut self) -> StructField {
        let name = self.expect_ident("expected a field name");
        let span = self.previous().span;
        self.expect(TokenDiscriminant::Colon, "expected `:` after field name");
        let ty = self.parse_type_ref("expected a type name after `:`");
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after struct field",
        );
        StructField { name, ty, span }
    }

    fn parse_enum_variants(&mut self, message: &'static str) -> Vec<EnumVariant> {
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline before an enum body",
        );
        self.expect(TokenDiscriminant::Indent, message);

        let mut variants = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            variants.push(self.parse_enum_variant());
        }

        self.expect(TokenDiscriminant::Dedent, "expected an enum body dedent");
        variants
    }

    fn parse_enum_variant(&mut self) -> EnumVariant {
        let name = self.expect_ident("expected an enum variant name");
        let start = self.previous().span;
        let mut end = start;
        let mut fields = Vec::new();
        if self.matches(TokenDiscriminant::LParen) {
            loop {
                let field_name = self.expect_ident("expected an enum payload field name");
                let field_span = self.previous().span;
                self.expect(
                    TokenDiscriminant::Colon,
                    "expected `:` after the enum payload field name",
                );
                let ty = self.parse_type_ref("expected a type name after `:`");
                fields.push(EnumVariantField {
                    name: field_name,
                    ty,
                    span: field_span,
                });
                if !self.matches(TokenDiscriminant::Comma) {
                    break;
                }
            }
            end = self.expect(
                TokenDiscriminant::RParen,
                "expected `)` after enum payload fields",
            );
        }
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after enum variant",
        );
        EnumVariant {
            name,
            fields,
            span: Span::new(start.line, start.column, end.end_column),
        }
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

        if self.matches(TokenDiscriminant::If) {
            let span = self.previous().span;
            let condition = self.parse_expr();
            self.expect(
                TokenDiscriminant::Colon,
                "expected `:` after `if` condition",
            );
            let then_body = self.parse_block("expected an indented block after `if`");
            let else_body = if self.matches(TokenDiscriminant::Else) {
                self.expect(TokenDiscriminant::Colon, "expected `:` after `else`");
                self.parse_block("expected an indented block after `else`")
            } else {
                Vec::new()
            };
            return Stmt::If {
                condition,
                then_body,
                else_body,
                span,
            };
        }

        if self.matches(TokenDiscriminant::While) {
            let span = self.previous().span;
            let condition = self.parse_expr();
            self.expect(
                TokenDiscriminant::Colon,
                "expected `:` after `while` condition",
            );
            let body = self.parse_block("expected an indented block after `while`");
            return Stmt::While {
                condition,
                body,
                span,
            };
        }

        if self.matches(TokenDiscriminant::For) {
            let span = self.previous().span;
            let binding = self.expect_ident("expected a loop binding after `for`");
            self.expect(
                TokenDiscriminant::In,
                "expected `in` after the loop binding",
            );
            let iterable = self.parse_expr();
            self.expect(
                TokenDiscriminant::Colon,
                "expected `:` after the `for` iterable",
            );
            let body = self.parse_block("expected an indented block after `for`");
            return Stmt::For {
                binding,
                iterable,
                body,
                span,
            };
        }

        if self.matches(TokenDiscriminant::Break) {
            let span = self.previous().span;
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after `break`",
            );
            return Stmt::Break(span);
        }

        if self.matches(TokenDiscriminant::Continue) {
            let span = self.previous().span;
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after `continue`",
            );
            return Stmt::Continue(span);
        }

        if self.matches(TokenDiscriminant::Match) {
            let span = self.previous().span;
            let value = self.parse_expr();
            self.expect(TokenDiscriminant::Colon, "expected `:` after `match` value");
            let arms = self.parse_match_arms("expected an indented block after `match`");
            return Stmt::Match { value, arms, span };
        }

        if self.matches(TokenDiscriminant::Select) {
            let span = self.previous().span;
            self.expect(TokenDiscriminant::Colon, "expected `:` after `select`");
            let arms = self.parse_select_arms("expected an indented block after `select`");
            return Stmt::Select { arms, span };
        }

        if self.matches(TokenDiscriminant::Mut) {
            let span = self.previous().span;
            let name = self.expect_ident("expected an identifier after `mut`");
            let ty = self.parse_optional_type_ref();
            self.expect(TokenDiscriminant::Equal, "expected `=` in a binding");
            let value = self.parse_expr();
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after binding",
            );
            return Stmt::Bind {
                name,
                mutable: true,
                ty,
                value,
                span,
            };
        }

        if self.check_ident_binding_or_assignment() {
            let name = self.expect_ident("expected a binding target");
            let span = self.previous().span;
            let ty = self.parse_optional_type_ref();
            self.expect(
                TokenDiscriminant::Equal,
                "expected `=` in an assignment or binding",
            );
            let value = self.parse_expr();
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after assignment",
            );
            return if ty.is_some() {
                Stmt::Bind {
                    name,
                    mutable: false,
                    ty,
                    value,
                    span,
                }
            } else {
                Stmt::Assign { name, value, span }
            };
        }

        let expr = self.parse_expr();
        let span = expr.span();
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after expression",
        );
        Stmt::Expr(expr, span)
    }

    fn parse_block(&mut self, message: &'static str) -> Vec<Stmt> {
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline before a block",
        );
        self.expect(TokenDiscriminant::Indent, message);

        let mut body = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            body.push(self.parse_stmt());
        }

        self.expect(TokenDiscriminant::Dedent, "expected a block dedent");
        body
    }

    fn parse_match_arms(&mut self, message: &'static str) -> Vec<MatchArm> {
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline before a match body",
        );
        self.expect(TokenDiscriminant::Indent, message);

        let mut arms = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            arms.push(self.parse_match_arm());
        }

        self.expect(TokenDiscriminant::Dedent, "expected a match body dedent");
        arms
    }

    fn parse_match_arm(&mut self) -> MatchArm {
        let pattern = self.parse_match_pattern();
        let span = pattern.span();
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after match arm pattern",
        );
        let body = self.parse_block("expected an indented block after match arm");
        MatchArm {
            pattern,
            body,
            span,
        }
    }

    fn parse_match_pattern(&mut self) -> MatchPattern {
        let enum_name = self.expect_ident("expected an enum name in match arm pattern");
        let start = self.previous().span;
        self.expect(
            TokenDiscriminant::Dot,
            "expected `.` after the enum name in match arm pattern",
        );
        let variant = self.expect_ident("expected an enum variant name in match arm pattern");
        let mut end = self.previous().span;
        let mut bindings = Vec::new();
        if self.matches(TokenDiscriminant::LParen) {
            loop {
                bindings.push(
                    self.expect_ident("expected a binding name in the payload match pattern"),
                );
                if !self.matches(TokenDiscriminant::Comma) {
                    break;
                }
            }
            end = self.expect(
                TokenDiscriminant::RParen,
                "expected `)` after the payload match bindings",
            );
        }

        MatchPattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            span: Span::new(start.line, start.column, end.end_column),
        }
    }

    fn parse_select_arms(&mut self, message: &'static str) -> Vec<SelectArm> {
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline before a select body",
        );
        self.expect(TokenDiscriminant::Indent, message);

        let mut arms = Vec::new();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            arms.push(self.parse_select_arm());
        }

        self.expect(TokenDiscriminant::Dedent, "expected a select body dedent");
        arms
    }

    fn parse_select_arm(&mut self) -> SelectArm {
        if self.matches(TokenDiscriminant::Default) {
            let span = self.previous().span;
            self.expect(
                TokenDiscriminant::Colon,
                "expected `:` after `default` select arm",
            );
            let body = self.parse_block("expected an indented block after a select arm");
            return SelectArm {
                binding: None,
                kind: SelectArmKind::Default,
                body,
                span,
            };
        }

        let binding = if self.check_select_binding() {
            let name = self.expect_ident("expected a binding name at the start of a select arm");
            self.expect(
                TokenDiscriminant::Equal,
                "expected `=` after the select arm binding name",
            );
            Some(name)
        } else {
            None
        };

        let operation = self.parse_expr();
        let span = operation.span();
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after select arm operation",
        );
        let body = self.parse_block("expected an indented block after a select arm");
        SelectArm {
            binding,
            kind: SelectArmKind::Operation { operation },
            body,
            span,
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_and();

        while self.matches(TokenDiscriminant::Or) {
            let rhs = self.parse_and();
            let span = Span::new(expr.span().line, expr.span().column, rhs.span().end_column);
            expr = Expr::Binary {
                lhs: Box::new(expr),
                op: BinaryOp::Or,
                rhs: Box::new(rhs),
                span,
            };
        }

        expr
    }

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_not();

        while self.matches(TokenDiscriminant::And) {
            let rhs = self.parse_not();
            let span = Span::new(expr.span().line, expr.span().column, rhs.span().end_column);
            expr = Expr::Binary {
                lhs: Box::new(expr),
                op: BinaryOp::And,
                rhs: Box::new(rhs),
                span,
            };
        }

        expr
    }

    fn parse_not(&mut self) -> Expr {
        if self.matches(TokenDiscriminant::Not) {
            let start = self.previous().span;
            let value = self.parse_not();
            return Expr::Unary {
                op: UnaryOp::Not,
                span: Span::new(start.line, start.column, value.span().end_column),
                value: Box::new(value),
            };
        }

        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_additive();

        loop {
            let op = if self.matches(TokenDiscriminant::EqualEqual) {
                Some(BinaryOp::Eq)
            } else if self.matches(TokenDiscriminant::BangEqual) {
                Some(BinaryOp::Ne)
            } else if self.matches(TokenDiscriminant::LessEqual) {
                Some(BinaryOp::Le)
            } else if self.matches(TokenDiscriminant::Less) {
                Some(BinaryOp::Lt)
            } else if self.matches(TokenDiscriminant::GreaterEqual) {
                Some(BinaryOp::Ge)
            } else if self.matches(TokenDiscriminant::Greater) {
                Some(BinaryOp::Gt)
            } else {
                None
            };

            let Some(op) = op else { break };
            let rhs = self.parse_additive();
            let span = Span::new(expr.span().line, expr.span().column, rhs.span().end_column);
            expr = Expr::Binary {
                lhs: Box::new(expr),
                op,
                rhs: Box::new(rhs),
                span,
            };
        }

        expr
    }

    fn parse_additive(&mut self) -> Expr {
        let mut expr = self.parse_multiplicative();

        while self.matches(TokenDiscriminant::Plus) || self.matches(TokenDiscriminant::Minus) {
            let operator = if self.previous_kind_matches(TokenDiscriminant::Plus) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let rhs = self.parse_multiplicative();
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

    fn parse_multiplicative(&mut self) -> Expr {
        let mut expr = self.parse_unary();

        while self.matches(TokenDiscriminant::Star)
            || self.matches(TokenDiscriminant::Slash)
            || self.matches(TokenDiscriminant::Percent)
        {
            let op = if self.previous_kind_matches(TokenDiscriminant::Star) {
                BinaryOp::Mul
            } else if self.previous_kind_matches(TokenDiscriminant::Slash) {
                BinaryOp::Div
            } else {
                BinaryOp::Mod
            };
            let rhs = self.parse_unary();
            let span = Span::new(expr.span().line, expr.span().column, rhs.span().end_column);
            expr = Expr::Binary {
                lhs: Box::new(expr),
                op,
                rhs: Box::new(rhs),
                span,
            };
        }

        expr
    }

    fn parse_unary(&mut self) -> Expr {
        if self.matches(TokenDiscriminant::Go) {
            let start = self.previous().span;
            let value = self.parse_unary();
            return Expr::Go {
                span: Span::new(start.line, start.column, value.span().end_column),
                value: Box::new(value),
            };
        }

        if self.matches(TokenDiscriminant::Await) {
            let start = self.previous().span;
            let value = self.parse_unary();
            return Expr::Await {
                span: Span::new(start.line, start.column, value.span().end_column),
                value: Box::new(value),
            };
        }

        if self.matches(TokenDiscriminant::Minus) {
            let start = self.previous().span;
            let value = self.parse_unary();
            return Expr::Unary {
                op: UnaryOp::Neg,
                span: Span::new(start.line, start.column, value.span().end_column),
                value: Box::new(value),
            };
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();

        loop {
            if self.matches(TokenDiscriminant::LParen) {
                let callee = match expr {
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
                expr = Expr::Call {
                    callee: callee.0,
                    args,
                    span: Span::new(callee.1.line, callee.1.column, end.end_column),
                };
                continue;
            }

            if self.matches(TokenDiscriminant::Dot) {
                let start = expr.span();
                let field = self.expect_member_name("expected a field name after `.`");
                if self.matches(TokenDiscriminant::LParen) {
                    let args = self.parse_args();
                    let end = self.expect(
                        TokenDiscriminant::RParen,
                        "expected `)` after method arguments",
                    );
                    expr = Expr::MethodCall {
                        target: Box::new(expr),
                        method: field,
                        args,
                        span: Span::new(start.line, start.column, end.end_column),
                    };
                } else {
                    let end = self.previous().span;
                    expr = Expr::Field {
                        target: Box::new(expr),
                        field,
                        span: Span::new(start.line, start.column, end.end_column),
                    };
                }
                continue;
            }

            if self.matches(TokenDiscriminant::LBracket) {
                let start = expr.span();
                let index = self.parse_expr();
                let end = self.expect(
                    TokenDiscriminant::RBracket,
                    "expected `]` after index expression",
                );
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                    span: Span::new(start.line, start.column, end.end_column),
                };
                continue;
            }

            if self.matches(TokenDiscriminant::Question) {
                let start = expr.span();
                let end = self.previous().span;
                expr = Expr::Propagate {
                    value: Box::new(expr),
                    span: Span::new(start.line, start.column, end.end_column),
                };
                continue;
            }

            break;
        }

        expr
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
            TokenKind::True => Expr::Bool(true, token.span),
            TokenKind::False => Expr::Bool(false, token.span),
            TokenKind::Ident(value) => Expr::Ident(value, token.span),
            TokenKind::LParen => {
                let expr = self.parse_expr();
                self.expect(
                    TokenDiscriminant::RParen,
                    "expected `)` to close grouped expression",
                );
                expr
            }
            TokenKind::LBracket => {
                let mut items = Vec::new();
                if !self.check(TokenDiscriminant::RBracket) {
                    loop {
                        items.push(self.parse_expr());
                        if !self.matches(TokenDiscriminant::Comma) {
                            break;
                        }
                    }
                }
                let end = self.expect(
                    TokenDiscriminant::RBracket,
                    "expected `]` after list literal",
                );
                Expr::List {
                    items,
                    span: Span::new(token.span.line, token.span.column, end.end_column),
                }
            }
            TokenKind::LBrace => {
                let mut entries = Vec::new();
                if !self.check(TokenDiscriminant::RBrace) {
                    loop {
                        let key = self.parse_expr();
                        self.expect(
                            TokenDiscriminant::Colon,
                            "expected `:` between dict key and value",
                        );
                        let value = self.parse_expr();
                        entries.push(DictEntry { key, value });
                        if !self.matches(TokenDiscriminant::Comma) {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenDiscriminant::RBrace, "expected `}` after dict literal");
                Expr::Dict {
                    entries,
                    span: Span::new(token.span.line, token.span.column, end.end_column),
                }
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2001",
                        "unexpected token in expression",
                        "expected an identifier, literal, bool, or grouped expression",
                        token.span,
                    )
                    .with_fix_it("replace this token with a valid expression"),
                );
                Expr::Ident("_error".to_string(), token.span)
            }
        }
    }

    fn parse_optional_type_ref(&mut self) -> Option<TypeRef> {
        if !self.matches(TokenDiscriminant::Colon) {
            return None;
        }

        Some(self.parse_type_ref("expected a type name after `:`"))
    }

    fn parse_optional_return_type_ref(&mut self) -> Option<TypeRef> {
        if !self.matches(TokenDiscriminant::Arrow) {
            return None;
        }

        Some(self.parse_type_ref("expected a type name after `->`"))
    }

    fn parse_type_ref(&mut self, message: &'static str) -> TypeRef {
        let name = self.expect_ident(message);
        let start = self.previous().span;
        let mut end = start;
        let mut args = Vec::new();

        if self.matches(TokenDiscriminant::LBracket) {
            loop {
                let arg = self.parse_type_ref("expected a type argument");
                args.push(arg);
                if !self.matches(TokenDiscriminant::Comma) {
                    break;
                }
            }

            end = self.expect(
                TokenDiscriminant::RBracket,
                "expected `]` after type arguments",
            );
        }

        TypeRef {
            name,
            args,
            span: Span::new(start.line, start.column, end.end_column),
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

    fn expect_member_name(&mut self, message: &'static str) -> String {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Ident(value) => value,
            TokenKind::True => "true".to_string(),
            TokenKind::False => "false".to_string(),
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2002",
                        message,
                        "member names must be identifiers or supported keyword literals like `true`/`false`",
                        token.span,
                    )
                    .with_fix_it("replace this token with a valid member name"),
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

    fn check_ident_binding_or_assignment(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && matches!(
                self.peek_next().map(|token| &token.kind),
                Some(TokenKind::Equal | TokenKind::Colon)
            )
    }

    fn check_select_binding(&self) -> bool {
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
            | Expr::Bool(_, span)
            | Expr::Ident(_, span)
            | Expr::List { span, .. }
            | Expr::Dict { span, .. }
            | Expr::Call { span, .. }
            | Expr::Field { span, .. }
            | Expr::MethodCall { span, .. }
            | Expr::Index { span, .. }
            | Expr::Go { span, .. }
            | Expr::Await { span, .. }
            | Expr::Propagate { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Binary { span, .. } => *span,
        }
    }
}

impl MatchPattern {
    pub fn span(&self) -> Span {
        match self {
            MatchPattern::EnumVariant { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TokenDiscriminant {
    Module,
    Import,
    Struct,
    Enum,
    Test,
    Fixture,
    Fn,
    If,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Match,
    Select,
    Default,
    Go,
    Await,
    And,
    Or,
    Not,
    Return,
    Mut,
    LParen,
    RParen,
    LBracket,
    RBracket,
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

impl TokenDiscriminant {
    fn matches(self, kind: &TokenKind) -> bool {
        matches!(
            (self, kind),
            (Self::Module, TokenKind::Module)
                | (Self::Import, TokenKind::Import)
                | (Self::Struct, TokenKind::Struct)
                | (Self::Enum, TokenKind::Enum)
                | (Self::Test, TokenKind::Test)
                | (Self::Fixture, TokenKind::Fixture)
                | (Self::Fn, TokenKind::Fn)
                | (Self::If, TokenKind::If)
                | (Self::Else, TokenKind::Else)
                | (Self::While, TokenKind::While)
                | (Self::For, TokenKind::For)
                | (Self::In, TokenKind::In)
                | (Self::Break, TokenKind::Break)
                | (Self::Continue, TokenKind::Continue)
                | (Self::Match, TokenKind::Match)
                | (Self::Select, TokenKind::Select)
                | (Self::Default, TokenKind::Default)
                | (Self::Go, TokenKind::Go)
                | (Self::Await, TokenKind::Await)
                | (Self::And, TokenKind::And)
                | (Self::Or, TokenKind::Or)
                | (Self::Not, TokenKind::Not)
                | (Self::Return, TokenKind::Return)
                | (Self::Mut, TokenKind::Mut)
                | (Self::LParen, TokenKind::LParen)
                | (Self::RParen, TokenKind::RParen)
                | (Self::LBracket, TokenKind::LBracket)
                | (Self::RBracket, TokenKind::RBracket)
                | (Self::RBrace, TokenKind::RBrace)
                | (Self::Dot, TokenKind::Dot)
                | (Self::Colon, TokenKind::Colon)
                | (Self::Comma, TokenKind::Comma)
                | (Self::Equal, TokenKind::Equal)
                | (Self::EqualEqual, TokenKind::EqualEqual)
                | (Self::BangEqual, TokenKind::BangEqual)
                | (Self::Less, TokenKind::Less)
                | (Self::LessEqual, TokenKind::LessEqual)
                | (Self::Greater, TokenKind::Greater)
                | (Self::GreaterEqual, TokenKind::GreaterEqual)
                | (Self::Question, TokenKind::Question)
                | (Self::Arrow, TokenKind::Arrow)
                | (Self::Plus, TokenKind::Plus)
                | (Self::Minus, TokenKind::Minus)
                | (Self::Star, TokenKind::Star)
                | (Self::Slash, TokenKind::Slash)
                | (Self::Percent, TokenKind::Percent)
                | (Self::Newline, TokenKind::Newline)
                | (Self::Indent, TokenKind::Indent)
                | (Self::Dedent, TokenKind::Dedent)
                | (Self::Eof, TokenKind::Eof)
        )
    }

    fn as_hint(self) -> &'static str {
        match self {
            Self::Module => "`module`",
            Self::Import => "`import`",
            Self::Struct => "`struct`",
            Self::Enum => "`enum`",
            Self::Fixture => "`fixture`",
            Self::Fn => "`fn`",
            Self::If => "`if`",
            Self::Else => "`else`",
            Self::While => "`while`",
            Self::For => "`for`",
            Self::In => "`in`",
            Self::Break => "`break`",
            Self::Continue => "`continue`",
            Self::Match => "`match`",
            Self::Select => "`select`",
            Self::Default => "`default`",
            Self::Go => "`go`",
            Self::Await => "`await`",
            Self::And => "`and`",
            Self::Or => "`or`",
            Self::Not => "`not`",
            Self::Return => "`return`",
            Self::Test => "`test`",
            Self::Mut => "`mut`",
            Self::LParen => "`(`",
            Self::RParen => "`)`",
            Self::LBracket => "`[`",
            Self::RBracket => "`]`",
            Self::RBrace => "`}`",
            Self::Dot => "`.`",
            Self::Colon => "`:`",
            Self::Comma => "`,`",
            Self::Equal => "`=`",
            Self::EqualEqual => "`==`",
            Self::BangEqual => "`!=`",
            Self::Less => "`<`",
            Self::LessEqual => "`<=`",
            Self::Greater => "`>`",
            Self::GreaterEqual => "`>=`",
            Self::Question => "`?`",
            Self::Arrow => "`->`",
            Self::Plus => "`+`",
            Self::Minus => "`-`",
            Self::Star => "`*`",
            Self::Slash => "`/`",
            Self::Percent => "`%`",
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
        TokenKind::True => "`true`".to_string(),
        TokenKind::False => "`false`".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{BinaryOp, Expr, FunctionKind, MatchPattern, SelectArmKind, Stmt, UnaryOp, parse};
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;

    #[test]
    fn parses_function_module() {
        let source = SourceFile::new("test.gof", "import math\n\nfn main():\n    return 40 + 2\n");
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.imports[0].module, "math");
        assert!(module.structs.is_empty());
        assert!(module.enums.is_empty());
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }

    #[test]
    fn parses_bindings_and_calls() {
        let source = SourceFile::new(
            "test.gof",
            "fn add(a: int, b: int) -> int:\n    return a + b\nfn main() -> int:\n    mut total: int = add(40, 1)\n    total = total + 1\n    return total\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.functions.len(), 2);
        assert_eq!(
            module.functions[0].params[0].ty.as_ref().unwrap().name,
            "int"
        );
        assert_eq!(
            module.functions[0].return_type.as_ref().unwrap().name,
            "int"
        );
        assert!(matches!(
            &module.functions[1].body[0],
            Stmt::Bind {
                mutable: true,
                ty: Some(_),
                ..
            }
        ));
        assert!(matches!(
            &module.functions[1].body[2],
            Stmt::Return(Expr::Ident(name, _), _) if name == "total"
        ));
    }

    #[test]
    fn parses_parameterized_builtin_type_refs() {
        let source = SourceFile::new(
            "test.gof",
            "fn main(values: list[int], jobs: task[list[string]]) -> dict[int]:\n    mut ch: channel[int] = channel()\n    send(ch, values[0])\n    return {\"ok\": recv(ch)}\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let function = &module.functions[0];

        let values_ty = function.params[0]
            .ty
            .as_ref()
            .expect("values type should exist");
        assert_eq!(values_ty.name, "list");
        assert_eq!(values_ty.args.len(), 1);
        assert_eq!(values_ty.args[0].name, "int");

        let jobs_ty = function.params[1]
            .ty
            .as_ref()
            .expect("jobs type should exist");
        assert_eq!(jobs_ty.name, "task");
        assert_eq!(jobs_ty.args[0].name, "list");
        assert_eq!(jobs_ty.args[0].args[0].name, "string");

        let return_ty = function
            .return_type
            .as_ref()
            .expect("return type should exist");
        assert_eq!(return_ty.name, "dict");
        assert_eq!(return_ty.args[0].name, "int");

        match &function.body[0] {
            Stmt::Bind { ty, .. } => {
                let ty = ty.as_ref().expect("binding type should exist");
                assert_eq!(ty.name, "channel");
                assert_eq!(ty.args[0].name, "int");
            }
            other => panic!("expected typed bind, got {other:?}"),
        }
    }

    #[test]
    fn parses_explicit_return_annotations() {
        let source = SourceFile::new(
            "test.gof",
            "fn truth() -> bool:\n    return true\nfn main() -> bool:\n    return truth()\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(
            module.functions[0].return_type.as_ref().unwrap().name,
            "bool"
        );
        assert_eq!(
            module.functions[1].return_type.as_ref().unwrap().name,
            "bool"
        );
    }

    #[test]
    fn parses_if_else_and_while() {
        let source = SourceFile::new(
            "test.gof",
            "fn main():\n    mut x = 3\n    while x > 0:\n        x = x - 1\n    if x == 0:\n        return true\n    else:\n        return false\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(&module.functions[0].body[1], Stmt::While { .. }));
        assert!(matches!(&module.functions[0].body[2], Stmt::If { .. }));
    }

    #[test]
    fn parses_for_in_loops() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return total\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::For { binding, .. } if binding == "value"
        ));
    }

    #[test]
    fn parses_break_and_continue() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");

        match &module.functions[0].body[1] {
            Stmt::For { body, .. } => {
                assert!(matches!(
                    &body[0],
                    Stmt::If { then_body, .. } if matches!(&then_body[0], Stmt::Continue(_))
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::If { then_body, .. } if matches!(&then_body[0], Stmt::Break(_))
                ));
            }
            other => panic!("expected for loop, got {other:?}"),
        }
    }

    #[test]
    fn parses_go_and_await() {
        let source = SourceFile::new(
            "test.gof",
            "fn square(x):\n    return x * x\nfn main():\n    task = go square(12)\n    return await task\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[1].body[0],
            Stmt::Assign {
                value: Expr::Go { .. },
                ..
            }
        ));
        assert!(matches!(
            &module.functions[1].body[1],
            Stmt::Return(Expr::Await { .. }, _)
        ));
    }

    #[test]
    fn parses_list_literals_and_indexing() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    values = [1, 2, 3]\n    return values[1]\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Assign {
                value: Expr::List { .. },
                ..
            }
        ));
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Return(Expr::Index { .. }, _)
        ));
    }

    #[test]
    fn parses_dict_literals_and_indexing() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    values = {\"ok\": 2, \"warn\": 3}\n    return values[\"ok\"]\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Assign {
                value: Expr::Dict { entries, .. },
                ..
            } if entries.len() == 2
        ));
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Return(Expr::Index { .. }, _)
        ));
    }

    #[test]
    fn parses_structs_and_field_access() {
        let source = SourceFile::new(
            "test.gof",
            "struct Point:\n    x: int\n    y: int\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.x + point.y\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.structs.len(), 1);
        assert_eq!(module.structs[0].name, "Point");
        assert_eq!(module.structs[0].fields.len(), 2);
        assert_eq!(module.structs[0].fields[0].name, "x");
        assert_eq!(module.structs[0].fields[0].ty.name, "int");
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Return(
                Expr::Binary {
                    lhs,
                    rhs,
                    ..
                },
                _
            ) if matches!(lhs.as_ref(), Expr::Field { field, .. } if field == "x")
                && matches!(rhs.as_ref(), Expr::Field { field, .. } if field == "y")
        ));
    }

    #[test]
    fn parses_enums_and_variant_references() {
        let source = SourceFile::new(
            "test.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> bool:\n    return Status.Ready == Status.Busy\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(module.enums.len(), 1);
        assert_eq!(module.enums[0].name, "Status");
        assert_eq!(module.enums[0].variants.len(), 2);
        assert_eq!(module.enums[0].variants[0].name, "Ready");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Return(
                Expr::Binary {
                    lhs,
                    rhs,
                    op: BinaryOp::Eq,
                    ..
                },
                _
            ) if matches!(lhs.as_ref(), Expr::Field { field, .. } if field == "Ready")
                && matches!(rhs.as_ref(), Expr::Field { field, .. } if field == "Busy")
        ));
    }

    #[test]
    fn parses_payload_enums_and_destructuring_patterns() {
        let source = SourceFile::new(
            "test.gof",
            "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn main() -> int:\n    state = JobState.Running(42)\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");

        assert_eq!(module.enums[0].variants[1].name, "Running");
        assert_eq!(module.enums[0].variants[1].fields.len(), 1);
        assert_eq!(module.enums[0].variants[1].fields[0].name, "pid");
        assert_eq!(module.enums[0].variants[1].fields[0].ty.name, "int");

        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Assign {
                value: Expr::MethodCall { method, args, .. },
                ..
            } if method == "Running" && args.len() == 1
        ));

        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Match { arms, .. }
                if matches!(
                    &arms[1].pattern,
                    MatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "JobState" && variant == "Running" && bindings == &vec!["pid".to_string()]
                )
                && matches!(
                    &arms[2].pattern,
                    MatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "JobState" && variant == "Failed" && bindings == &vec!["message".to_string()]
                )
        ));
    }

    #[test]
    fn parses_match_arms_over_enum_variants() {
        let source = SourceFile::new(
            "test.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> int:\n    status: Status = Status.Ready\n    match status:\n        Status.Ready:\n            return 1\n        Status.Busy:\n            return 2\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Match { arms, .. } if arms.len() == 2
        ));
    }

    #[test]
    fn parses_receiver_methods_and_method_calls() {
        let source = SourceFile::new(
            "test.gof",
            "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert_eq!(
            module.functions[0].receiver_type.as_ref().unwrap().name,
            "Point"
        );
        assert_eq!(module.functions[0].name, "total");
        assert!(matches!(
            &module.functions[1].body[1],
            Stmt::Return(Expr::MethodCall { method, args, .. }, _) if method == "total" && args.len() == 1
        ));
    }

    #[test]
    fn parses_test_functions() {
        let source = SourceFile::new(
            "math_test.gof",
            "import testing\n\ntest fn truthy_case(t: TestContext):\n    t.true(true, \"expected truth\")\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");

        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].kind, FunctionKind::Test);
        assert_eq!(module.functions[0].name, "truthy_case");
    }

    #[test]
    fn parses_fixture_functions() {
        let source = SourceFile::new(
            "math_test.gof",
            "fixture(module) fn shared_total() -> int:\n    return 41\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");

        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].kind, FunctionKind::Fixture);
        assert_eq!(
            module.functions[0]
                .fixture_scope
                .as_ref()
                .expect("fixture scope should be recorded")
                .name,
            "module"
        );
        assert_eq!(module.functions[0].name, "shared_total");
    }

    #[test]
    fn parses_select_arms_with_optional_binding() {
        let source = SourceFile::new(
            "test.gof",
            "fn worker(ch):\n    return 1\nfn main() -> int:\n    left = channel()\n    right = channel()\n    select:\n        value = recv(left):\n            return value\n        recv(right):\n            return 2\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[1].body[2],
            Stmt::Select { arms, .. } if arms.len() == 2 && arms[0].binding.as_deref() == Some("value") && arms[1].binding.is_none()
        ));
        if let Stmt::Select { arms, .. } = &module.functions[1].body[2] {
            assert!(matches!(arms[0].kind, SelectArmKind::Operation { .. }));
            assert!(matches!(arms[1].kind, SelectArmKind::Operation { .. }));
        }
    }

    #[test]
    fn parses_select_send_arms() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    ch = channel(1)\n    select:\n        sent = send(ch, 7):\n            return 1\n        send(ch, 8):\n            return 2\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[1],
            Stmt::Select { arms, .. }
                if arms.len() == 2
                    && arms[0].binding.as_deref() == Some("sent")
                    && arms[1].binding.is_none()
        ));
        if let Stmt::Select { arms, .. } = &module.functions[0].body[1] {
            assert!(matches!(arms[0].kind, SelectArmKind::Operation { .. }));
            assert!(matches!(arms[1].kind, SelectArmKind::Operation { .. }));
        }
    }

    #[test]
    fn parses_select_default_arm() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    select:\n        default:\n            return 1\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Select { arms, .. }
                if arms.len() == 1
                    && arms[0].binding.is_none()
                    && matches!(arms[0].kind, SelectArmKind::Default)
        ));
    }

    #[test]
    fn parses_logical_operators_with_precedence() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> bool:\n    return not false and true or false\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Return(
                Expr::Binary {
                    op: BinaryOp::Or,
                    lhs,
                    ..
                },
                _
            ) if matches!(lhs.as_ref(), Expr::Binary { op: BinaryOp::And, .. })
        ));
    }

    #[test]
    fn parses_unary_minus_and_numeric_surface() {
        let source = SourceFile::new(
            "test.gof",
            "fn main() -> int:\n    return -6 / 3 % 2 + -(1 + 1)\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        assert!(matches!(
            &module.functions[0].body[0],
            Stmt::Return(
                Expr::Binary {
                    op: BinaryOp::Add,
                    lhs,
                    rhs,
                    ..
                },
                _
            ) if matches!(
                lhs.as_ref(),
                Expr::Binary {
                    op: BinaryOp::Mod,
                    lhs,
                    ..
                } if matches!(lhs.as_ref(), Expr::Binary { op: BinaryOp::Div, .. })
            ) && matches!(rhs.as_ref(), Expr::Unary { op: UnaryOp::Neg, .. })
        ));
    }

    #[test]
    fn parses_result_annotations_and_propagation() {
        let source = SourceFile::new(
            "test.gof",
            "fn parse_port() -> Result[int, string]:\n    return Result.Ok(41)\n\nfn main() -> Result[int, string]:\n    port = parse_port()?\n    return Result.Ok(port + 1)\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");

        let parse_return = module.functions[0]
            .return_type
            .as_ref()
            .expect("parse_port should have a return annotation");
        assert_eq!(parse_return.name, "Result");
        assert_eq!(parse_return.args.len(), 2);
        assert_eq!(parse_return.args[0].name, "int");
        assert_eq!(parse_return.args[1].name, "string");

        assert_eq!(module.functions[1].body.len(), 2);
    }
}
