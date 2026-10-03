use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use crate::token::{Token, TokenKind};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub imports: Vec<Import>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub functions: Vec<Function>,
    /// RFC 0044 protocol declarations.
    pub protocols: Vec<ProtocolDecl>,
    /// RFC 0044 explicit conformance declarations: `impl Type: Proto`.
    pub impls: Vec<ImplDecl>,
    /// RFC 0053 export block: `module:` as the FIRST top-level
    /// declaration, one bare identifier per line. Empty when the file
    /// is blockless (the grandfather window).
    pub exports: Vec<ModuleExport>,
    /// RFC 0054 const declarations: `const NAME: Type = expr`, top
    /// level only, mandatory annotation and initializer.
    pub consts: Vec<ConstDecl>,
}

/// RFC 0054: a top-level const declaration with its mandatory type
/// annotation and initializer. Evaluation happens once in typed HIR.
#[derive(Debug, Clone, Serialize)]
pub struct ConstDecl {
    pub name: String,
    pub ty: TypeRef,
    pub value: Expr,
    pub span: Span,
    /// Source file that declared the const — threaded for
    /// file-aware diagnostics (RFC 0054 GOF3173–3177).
    pub source_path: PathBuf,
}

/// One `module:` export entry — a bare identifier naming a top-level
/// declaration of the same file.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleExport {
    pub name: String,
    pub span: Span,
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
    /// RFC 0044 protocol-constrained type parameters: the `[T: Shape]`
    /// list before the value parameters. Empty for non-generic functions.
    pub type_params: Vec<TypeParam>,
    pub return_type: Option<TypeRef>,
    pub body: Vec<Stmt>,
    pub span: Span,
    pub source_path: PathBuf,
}

/// RFC 0044 type parameter with one or more protocol bounds.
#[derive(Debug, Clone, Serialize)]
pub struct TypeParam {
    pub name: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

/// RFC 0044 protocol declaration: `protocol Name:` followed by an
/// indented block of signature-only method requirements.
#[derive(Debug, Clone, Serialize)]
pub struct ProtocolDecl {
    pub name: String,
    pub requirements: Vec<ProtocolRequirement>,
    pub span: Span,
    pub source_path: PathBuf,
}

/// One signature-only requirement line inside a protocol block. The
/// receiver is the first parameter, annotated `Self`; a mutating
/// requirement writes it as `mut receiver: Self`, so receiver
/// mutability is `params[0].mutable`.
#[derive(Debug, Clone, Serialize)]
pub struct ProtocolRequirement {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub span: Span,
}

/// RFC 0044 explicit conformance declaration: `impl Struct: Proto[, ...]`
/// — block-less, newline-terminated, top level.
#[derive(Debug, Clone, Serialize)]
pub struct ImplDecl {
    pub struct_name: String,
    pub protocols: Vec<String>,
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

/// One function parameter. `mutable` is RFC 0044's `mut` parameter
/// marker: `mut self: Counter` method receivers and `mut counter: T`
/// protocol-constrained generic parameters. The parser accepts the
/// marker in named-function parameter lists; the typed layer owns the
/// mutability discipline (receiver conformance matching and
/// mutating-requirement callability).
#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub ty: Option<TypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>,
    pub span: Span,
}

/// A literal value in pattern position (RFC 0052). Int literals arrive
/// with their optional leading `-` folded in (the lexer keeps no raw
/// text, so `Minus` + `IntLiteral` is collapsed by the parser).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PatternLiteral {
    Int(i64),
    String(String),
    Bool(bool),
}

/// A destructuring target. `Name` is today's single-identifier binding;
/// `Tuple` is the RFC 0050 tuple pattern, shared by binding statements,
/// loop bindings, and match payload positions; `Wildcard` is the RFC
/// 0052 `_` leaf (matches anything, binds nothing); `Literal` is the
/// RFC 0052 refinement leaf (int, string, or bool value test).
#[derive(Debug, Clone, Serialize)]
pub enum BindingPattern {
    Name(String, Span),
    Tuple(Vec<BindingPattern>, Span),
    Wildcard(Span),
    Literal(PatternLiteral, Span),
}

impl BindingPattern {
    pub fn span(&self) -> Span {
        match self {
            BindingPattern::Name(_, span)
            | BindingPattern::Tuple(_, span)
            | BindingPattern::Wildcard(span)
            | BindingPattern::Literal(_, span) => *span,
        }
    }

    /// Every leaf binding name in this pattern, left to right. Used by
    /// layers that still lower patterns to flat name lists (HIR, typed
    /// HIR, MIR) while tuple destructuring semantics land layer by layer.
    /// Wildcard and literal leaves bind nothing and contribute no names.
    pub fn binding_names(&self) -> Vec<(String, Span)> {
        let mut names = Vec::new();
        self.collect_names(&mut names);
        names
    }

    fn collect_names(&self, names: &mut Vec<(String, Span)>) {
        match self {
            BindingPattern::Name(name, span) => names.push((name.clone(), *span)),
            BindingPattern::Tuple(leafs, _) => {
                for leaf in leafs {
                    leaf.collect_names(names);
                }
            }
            BindingPattern::Wildcard(_) | BindingPattern::Literal(_, _) => {}
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Stmt {
    Return(Expr, Span),
    Bind {
        pattern: BindingPattern,
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
        pattern: BindingPattern,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Defer(Expr, Span),
    /// RFC 0046 `unsafe:` block statement: permission region, ordinary
    /// block semantics, new lexical scope. `GOF3139` rejects it inside
    /// closure bodies; `GOF3141` warns that it is inert in v1.
    Unsafe(Vec<Stmt>, Span),
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
    /// RFC 0052 guard: `pattern if <bool expr>:` — legal on match arms
    /// only. Evaluated after the pattern matched and its bindings were
    /// created; a `false` guard falls through to the next arm.
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum MatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<BindingPattern>,
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
    Tuple {
        items: Vec<Expr>,
        span: Span,
    },
    TupleIndex {
        target: Box<Expr>,
        index: usize,
        span: Span,
    },
    /// RFC 0047 anonymous function expression: `fn (x: int) -> int: <block>`.
    /// Parameters carry mandatory type annotations (the parser enforces
    /// GOF3142 for a missing annotation); `-> Ret` is optional and infers
    /// from the body's return paths in the typed layer. The block
    /// (Indent..Dedent) is part of the expression: parsing continues on
    /// the line hosting the Dedent.
    Closure {
        params: Vec<Param>,
        return_type: Option<TypeRef>,
        body: Vec<Stmt>,
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
    /// RFC 0047: set when the expression just parsed ended with a closure
    /// block (its Indent..Dedent consumed as part of the expression). The
    /// physical line break before that block was already consumed, so a
    /// statement ending in such an expression terminates at the Dedent
    /// instead of expecting a trailing newline (see `finish_stmt_line`).
    expr_ended_with_block: bool,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            current: 0,
            diagnostics: Diagnostics::default(),
            expr_ended_with_block: false,
        }
    }

    fn parse_module(&mut self) -> Module {
        let mut imports = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        let mut protocols = Vec::new();
        let mut impls = Vec::new();
        // RFC 0053: `module:` must be the file's FIRST top-level
        // declaration — tracked for the GOF3168 late-block rejection.
        let mut exports: Vec<ModuleExport> = Vec::new();
        let mut consts: Vec<ConstDecl> = Vec::new();
        let mut seen_top_level = false;
        while !self.at_end() {
            self.skip_newlines();
            if self.matches(TokenDiscriminant::Eof) {
                break;
            }
            // RFC 0053 export block: `module:` + indented bare-identifier
            // entries, one per line. Valid only as the first top-level
            // declaration and at most once per file (GOF3168).
            if self.check(TokenDiscriminant::Module) && self.peek_ahead_is(TokenDiscriminant::Colon)
            {
                let span = self.advance().span;
                self.advance(); // `:`
                if seen_top_level || !exports.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "GOF3168",
                            "the `module:` export block must be the first top-level declaration",
                            "the export list is the file's public contract and is read before the implementation",
                            span,
                        )
                        .with_fix_it("move the `module:` block to the top of the file")
                        .with_source_path(PathBuf::new()),
                    );
                    // Recover: parse and discard the block.
                    let _ = self.parse_block("expected an indented block after `module:`");
                    continue;
                }
                self.skip_newlines();
                exports = self.parse_module_exports(span);
                continue;
            }
            seen_top_level = true;
            if self.check(TokenDiscriminant::Const) {
                consts.push(self.parse_const());
            } else if self.check(TokenDiscriminant::Import) {
                imports.push(self.parse_import());
            } else if self.check(TokenDiscriminant::Struct) {
                structs.push(self.parse_struct());
            } else if self.check(TokenDiscriminant::Enum) {
                enums.push(self.parse_enum());
            } else if self.check(TokenDiscriminant::Protocol) {
                protocols.push(self.parse_protocol());
            } else if self.check(TokenDiscriminant::Impl) {
                impls.push(self.parse_impl());
            } else if self.check(TokenDiscriminant::Unsafe) {
                // RFC 0046 GOF3140: `unsafe fn`, `unsafe struct`, and any
                // other modifier misuse at declaration level. Report, skip
                // the modifier, and parse the underlying declaration so
                // its own diagnostics stay reachable.
                self.advance();
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3140",
                        "`unsafe` is a block statement in gof and never part of a signature",
                        "write `unsafe:` followed by an indented block inside a function body, or remove the modifier",
                        self.previous().span,
                    )
                    .with_fix_it("remove the `unsafe` modifier, leaving the valid declaration")
                    .with_source_path(PathBuf::new()),
                );
            } else if self.check(TokenDiscriminant::Async) {
                // RFC 0051 GOF3158: `async fn`, `async test fn`, `async
                // fixture(scope) fn`, and `async fn Type.method` are
                // reserved-modifier misuses. Report, skip the modifier,
                // and parse the underlying declaration so body
                // diagnostics stay reachable.
                self.advance();
                let span = self.previous().span;
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3158",
                        "`async` is reserved and cannot decorate function declarations",
                        "v1 concurrency is `go` + `await` / `await_result`: every function can already spawn, join, and block, so there is no restriction for `async fn` to lift; when the spawn-and-join intent applies, write `t = go f(...)` then `await t`",
                        span,
                    )
                    .with_fix_it("remove the `async` modifier, leaving the valid declaration")
                    .with_source_path(PathBuf::new()),
                );
            } else {
                functions.push(self.parse_function());
            }
        }
        Module {
            imports,
            structs,
            enums,
            functions,
            protocols,
            impls,
            exports,
            consts,
        }
    }

    /// RFC 0054: `const NAME: Type = expr` — top level only,
    /// newline-terminated, same shape class as `import`. Statement or
    /// body positions fall through to the existing token diagnostics.
    fn parse_const(&mut self) -> ConstDecl {
        let span = self.expect(TokenDiscriminant::Const, "expected `const`");
        let name = self.expect_ident("expected a const name after `const`");
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after the const name (the annotation is mandatory)",
        );
        let ty = self.parse_type_ref("expected a type annotation for the const");
        self.expect(
            TokenDiscriminant::Equal,
            "expected `=` followed by the const initializer",
        );
        let value = self.parse_expr();
        self.finish_stmt_line("expected a newline after the const declaration");
        ConstDecl {
            name,
            ty,
            value,
            span,
            source_path: PathBuf::new(),
        }
    }

    /// RFC 0053: the `module:` export block — bare identifier entries,
    /// one per line, newline-terminated, same indented-block shape as
    /// struct fields. Duplicate entries and non-identifier content
    /// report GOF3168; the block must contain at least one entry.
    fn parse_module_exports(&mut self, span: Span) -> Vec<ModuleExport> {
        let mut exports = Vec::new();
        let mut seen: HashMap<String, Span> = HashMap::new();
        if !self.check(TokenDiscriminant::Indent) {
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3168",
                    "the `module:` export block requires at least one entry",
                    "gof has no empty blocks; a module with nothing to export is not imported by anyone",
                    span,
                )
                .with_fix_it("add the exported names, one per line")
                .with_source_path(PathBuf::new()),
            );
            return exports;
        }
        self.advance();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenDiscriminant::Dedent) {
                break;
            }
            if self.matches(TokenDiscriminant::Eof) {
                break;
            }
            if !matches!(self.peek().kind, TokenKind::Ident(_)) {
                let bad = self.peek().span;
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3168",
                        "export entries are bare identifiers, one per line",
                        "no commas, no qualifiers, no trailing colons — the block shape matches struct fields",
                        bad,
                    )
                    .with_fix_it("write one exported name per line")
                    .with_source_path(PathBuf::new()),
                );
                self.advance();
                continue;
            }
            let name = self.expect_ident("expected an exported name");
            let entry_span = self.previous().span;
            if let Some(first) = seen.get(&name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3168",
                        format!("duplicate export entry `{name}`"),
                        format!("the entry was first declared at line {}", first.line),
                        entry_span,
                    )
                    .with_fix_it("remove the duplicate entry")
                    .with_source_path(PathBuf::new()),
                );
                continue;
            }
            seen.insert(name.clone(), entry_span);
            exports.push(ModuleExport {
                name,
                span: entry_span,
            });
            self.expect(
                TokenDiscriminant::Newline,
                "expected a newline after the export entry",
            );
        }
        self.expect(
            TokenDiscriminant::Dedent,
            "expected the export block to end",
        );
        exports
    }

    /// RFC 0044 protocol declaration: `protocol Name:` followed by an
    /// indented block of signature-only requirements. Each requirement is
    /// an `fn` line WITHOUT a body and WITHOUT a trailing colon,
    /// newline-terminated.
    fn parse_protocol(&mut self) -> ProtocolDecl {
        let span = self.expect(TokenDiscriminant::Protocol, "expected `protocol`");
        let name = self.expect_ident("expected a protocol name after `protocol`");
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after the protocol name",
        );
        self.skip_newlines();
        let requirements = self.parse_protocol_requirements();
        ProtocolDecl {
            name,
            requirements,
            span,
            source_path: PathBuf::new(),
        }
    }

    fn parse_protocol_requirements(&mut self) -> Vec<ProtocolRequirement> {
        let mut requirements = Vec::new();
        if !self.check(TokenDiscriminant::Indent) {
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3132",
                    "protocol blocks require at least one requirement",
                    "gof has no marker protocols: an empty protocol block has no use without existential values",
                    self.previous().span,
                )
                .with_fix_it("add an `fn` requirement inside the protocol block"),
            );
            return requirements;
        }
        self.advance();
        while !self.check(TokenDiscriminant::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenDiscriminant::Dedent) {
                break;
            }
            if self.matches(TokenDiscriminant::Eof) {
                break;
            }
            if !self.check(TokenDiscriminant::Fn) {
                // Only `fn` requirement lines are legal content inside a
                // protocol block; anything else (struct fields, nested
                // declarations, `test`/`fixture` forms, stray
                // expressions) is rejected here rather than parsed as a
                // declaration.
                let span = self.peek().span;
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3132",
                        "protocol blocks contain only `fn` requirements",
                        "a protocol block is a list of signature-only `fn` lines; no other declaration or statement is legal here",
                        span,
                    )
                    .with_fix_it("remove this line or move it out of the protocol block"),
                );
                // Recover past the offending line so the parser can keep
                // checking the rest of the module.
                while !self.matches(TokenDiscriminant::Newline) && !self.at_end() {
                    self.advance();
                }
                continue;
            }
            requirements.push(self.parse_protocol_requirement());
        }
        self.expect(
            TokenDiscriminant::Dedent,
            "expected the protocol block to end",
        );
        if requirements.is_empty() {
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3132",
                    "protocol blocks require at least one requirement",
                    "gof has no marker protocols: an empty protocol block has no use without existential values",
                    self.previous().span,
                )
                .with_fix_it("add an `fn` requirement inside the protocol block"),
            );
        }
        requirements
    }

    fn parse_protocol_requirement(&mut self) -> ProtocolRequirement {
        let span = self.expect(TokenDiscriminant::Fn, "expected an `fn` requirement");
        let name = self.expect_ident("expected a requirement name");
        // Requirements use the unqualified form: the conforming type is
        // unknown, so `fn Shape.area(...)` falsely suggests the protocol
        // owns or provides the method.
        if self.check(TokenDiscriminant::Dot) {
            let dot_span = self.advance().span;
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3132",
                    "protocol requirements are unqualified `fn` lines",
                    "a requirement has no owner to name: the conforming type is unknown and the method bodies live in conforming structs",
                    dot_span,
                )
                .with_fix_it(format!("write the requirement as `fn {name}(...)`")),
            );
            self.skip_to_newline();
            return ProtocolRequirement {
                name,
                params: Vec::new(),
                return_type: None,
                span,
            };
        }
        self.expect(
            TokenDiscriminant::LParen,
            "expected `(` after the requirement name",
        );
        let mut params = Vec::new();
        while !self.check(TokenDiscriminant::RParen) {
            params.push(self.parse_param());
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        self.expect(
            TokenDiscriminant::RParen,
            "expected `)` after the requirement parameters",
        );
        let return_type = if self.check(TokenDiscriminant::Arrow) {
            self.advance();
            Some(self.parse_type_ref("expected a return type for the requirement"))
        } else {
            None
        };
        // A requirement line is newline-terminated: a trailing colon is
        // gof's block introducer, and a requirement has nothing to
        // introduce. When a colon (and an indented body block) follows,
        // report GOF3132 and recover past the block so the rest of the
        // protocol still parses.
        if self.check(TokenDiscriminant::Colon) {
            let colon_span = self.advance().span;
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3132",
                    "protocol requirements carry no body",
                    "the trailing colon is gof's block introducer; a requirement is a signature-only line and the method bodies live in conforming structs",
                    colon_span,
                )
                .with_fix_it("remove the trailing colon from this requirement"),
            );
            if self.check(TokenDiscriminant::Newline) {
                self.advance();
                if self.check(TokenDiscriminant::Indent) {
                    let mut depth = 1usize;
                    self.advance();
                    while depth > 0 && !self.at_end() {
                        if self.matches(TokenDiscriminant::Indent) {
                            depth += 1;
                        } else if self.matches(TokenDiscriminant::Dedent) {
                            depth -= 1;
                        } else {
                            self.advance();
                        }
                    }
                    return ProtocolRequirement {
                        name,
                        params,
                        return_type,
                        span,
                    };
                }
            }
            return ProtocolRequirement {
                name,
                params,
                return_type,
                span,
            };
        }
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after the requirement",
        );
        ProtocolRequirement {
            name,
            params,
            return_type,
            span,
        }
    }

    /// Consume tokens through the end of the current physical line —
    /// parser recovery for malformed lines whose shape cannot be
    /// interpreted at all.
    fn skip_to_newline(&mut self) {
        while !self.check(TokenDiscriminant::Newline) && !self.at_end() {
            self.advance();
        }
        if self.check(TokenDiscriminant::Newline) {
            self.advance();
        }
    }

    /// RFC 0044 explicit conformance: `impl Struct: Proto[, Proto2]` —
    /// block-less, newline-terminated.
    fn parse_impl(&mut self) -> ImplDecl {
        let span = self.expect(TokenDiscriminant::Impl, "expected `impl`");
        let struct_name = self.expect_ident("expected a struct name after `impl`");
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` between the struct name and the protocol list",
        );
        let mut protocols = Vec::new();
        loop {
            protocols.push(self.expect_ident("expected a protocol name in the impl list"));
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        self.expect(
            TokenDiscriminant::Newline,
            "expected a newline after the impl declaration",
        );
        ImplDecl {
            struct_name,
            protocols,
            span,
            source_path: PathBuf::new(),
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
            self.expect(
                TokenDiscriminant::Fn,
                "expected `fn` after `fixture(scope)`",
            );
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
        // RFC 0044 protocol-constrained type parameters: `[T: Shape]`
        // before the value parameter list.
        let type_params = if self.check(TokenDiscriminant::LBracket) {
            self.parse_type_params()
        } else {
            Vec::new()
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
            type_params,
            return_type,
            body,
            span: Span::new(start.line, start.column, start.end_column),
            source_path: PathBuf::new(),
        }
    }

    /// RFC 0044: `[T: Shape, U: Serializable]` and `[T: Shape, Drawable]`
    /// — one or more type parameters, each with one or more protocol
    /// bounds. Every type parameter MUST carry a colon and at least one
    /// bound at parse time, which makes the comma list unambiguous
    /// without lookahead: a comma-separated identifier followed by `:`
    /// opens a new type parameter (bounds are bare names and never carry
    /// a colon), and any other identifier is another bound of the
    /// current one. `[T]` and `[T, U]` are rejected here with GOF3132.
    fn parse_type_params(&mut self) -> Vec<TypeParam> {
        self.expect(
            TokenDiscriminant::LBracket,
            "expected `[` to start the type parameter list",
        );
        let mut params: Vec<TypeParam> = Vec::new();
        let mut current: Option<TypeParam> = None;
        while !self.check(TokenDiscriminant::RBracket) && !self.at_end() {
            let name = self.expect_ident("expected a type parameter name or protocol bound");
            let span = self.previous().span;
            if self.matches(TokenDiscriminant::Colon) {
                // `Name:` opens a new type parameter; its first bound is
                // mandatory right after the colon.
                if let Some(param) = current.take() {
                    params.push(param);
                }
                let first_bound = self.expect_ident("expected a protocol name in the bounds list");
                current = Some(TypeParam {
                    name,
                    bounds: vec![first_bound],
                    span,
                });
            } else {
                // A bare identifier after a comma is another bound of the
                // current type parameter — unless there is no current
                // parameter, or the current one is itself an unbounded
                // error recovery, in which case this name is another
                // unbounded type parameter.
                let has_bounded_current = current
                    .as_ref()
                    .map(|param| !param.bounds.is_empty())
                    .unwrap_or(false);
                if has_bounded_current {
                    current.as_mut().expect("checked above").bounds.push(name);
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "GOF3132",
                            format!("type parameter `{name}` requires protocol bounds"),
                            "every type parameter in a generic function must carry a colon and at least one protocol bound, as in `[T: Shape]`",
                            span,
                        )
                        .with_fix_it(format!("write the parameter as `[{name}: Protocol]`")),
                    );
                    if let Some(param) = current.take() {
                        params.push(param);
                    }
                    current = Some(TypeParam {
                        name,
                        bounds: Vec::new(),
                        span,
                    });
                }
            }
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        if let Some(param) = current.take() {
            params.push(param);
        }
        if params.is_empty() {
            let span = self.previous().span;
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3132",
                    "type parameter lists cannot be empty",
                    "a generic function declares one or more type parameters, each with protocol bounds",
                    span,
                )
                .with_fix_it("add a type parameter such as `[T: Protocol]`"),
            );
        }
        self.expect(
            TokenDiscriminant::RBracket,
            "expected `]` to close the type parameter list",
        );
        params
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
        // RFC 0044: a `mut` marker opens an inout-view parameter —
        // `mut self: Counter` method receivers and `mut counter: T`
        // generic parameters. The typed layer owns the discipline; the
        // parser only records the marker, extending the span over the
        // whole `mut name` pair.
        let mut_span = self.check(TokenDiscriminant::Mut).then(|| self.peek().span);
        let mutable = self.matches(TokenDiscriminant::Mut);
        let name = self.expect_ident("expected parameter name");
        let span = match mut_span {
            Some(start) => Span::new(start.line, start.column, self.previous().span.end_column),
            None => self.previous().span,
        };
        let ty = self.parse_optional_type_ref();
        Param {
            name,
            mutable,
            ty,
            span,
        }
    }

    /// RFC 0047 closure parameters: same shape as named-function
    /// parameters, but every parameter requires an explicit type
    /// annotation (GOF3142), duplicate names are rejected (GOF3142), and
    /// `self` is rejected as a receiver-style parameter (GOF3142).
    /// RFC 0044 `mut` parameters are named-function surface only:
    /// closures have no receiver conformance story, so the marker is
    /// rejected here (GOF3142) instead of gaining meaning.
    fn parse_closure_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if self.check(TokenDiscriminant::RParen) {
            return params;
        }

        let mut seen = std::collections::HashSet::new();
        loop {
            if self.check(TokenDiscriminant::Mut) {
                let span = self.advance().span;
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3142",
                        "closure parameters cannot use `mut`",
                        "the `mut` parameter marker applies to named method receivers and protocol-constrained generic parameters; anonymous functions have neither",
                        span,
                    )
                    .with_fix_it("remove the `mut` marker or use a named function"),
                );
            }
            let name = self.expect_ident("expected a closure parameter name");
            let span = self.previous().span;
            if !seen.insert(name.clone()) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3142",
                        format!("duplicate closure parameter `{name}`"),
                        "anonymous function parameters must use unique names within the signature",
                        span,
                    )
                    .with_fix_it("rename or remove the duplicate parameter"),
                );
            }
            if name == "self" {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3142",
                        "closure parameters cannot use the receiver name `self`",
                        "anonymous functions have no receiver; `self` is reserved for named method declarations",
                        span,
                    )
                    .with_fix_it("rename the parameter to describe its value")
                );
            }
            let ty = if self.matches(TokenDiscriminant::Colon) {
                Some(self.parse_type_ref("expected a type after `:` in a closure parameter"))
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3142",
                        format!("closure parameter `{name}` requires a type annotation"),
                        "closure parameters require explicit type annotations",
                        span,
                    )
                    .with_fix_it(format!("annotate the parameter as `{name}: <type>`")),
                );
                None
            };
            params.push(Param {
                name,
                mutable: false,
                ty,
                span,
            });
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
        }
        params
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
        self.expr_ended_with_block = false;
        if self.matches(TokenDiscriminant::Return) {
            let span = self.previous().span;
            let expr = self.parse_expr();
            self.finish_stmt_line("expected a newline after `return`");
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
            // RFC 0050: `for (a, b) in pairs:` destructures each element.
            // Statements may begin with `(` in the loop-binding position, so
            // the tuple pattern is parsed speculatively: it is kept only when
            // a well-formed pattern is followed by `in`; otherwise the parser
            // rewinds and takes today's single-identifier binding path.
            // RFC 0052: `_` is a wildcard leaf (binds nothing) and guards
            // are rejected here with a specific parse error.
            let pattern = if self.check(TokenDiscriminant::LParen) {
                let token_mark = self.mark();
                let diagnostic_mark = self.diagnostics.0.len();
                let pattern = self.parse_binding_pattern();
                if self.check(TokenDiscriminant::If) {
                    // A guard cannot follow a `for` binding: keep the
                    // (already well-formed) pattern and report the
                    // guard placement, then recover past the guard.
                    self.reject_position_guard();
                    pattern
                } else if self.check(TokenDiscriminant::In) {
                    pattern
                } else {
                    // Rewind and restore the pre-tuples behavior: a
                    // parenthesized loop target is NOT a valid binding,
                    // so the identifier expectation reports the first
                    // error and the parser's normal recovery cascade
                    // follows — never a silently-wrong program.
                    self.reset(token_mark);
                    self.diagnostics.0.truncate(diagnostic_mark);
                    let name = self.expect_ident("expected a loop binding after `for`");
                    let name_span = self.previous().span;
                    self.reject_position_guard();
                    binding_leaf_from_name(name, name_span)
                }
            } else {
                let name = self.expect_ident("expected a loop binding after `for`");
                let name_span = self.previous().span;
                self.reject_position_guard();
                binding_leaf_from_name(name, name_span)
            };
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
                pattern,
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

        if self.matches(TokenDiscriminant::Defer) {
            let span = self.previous().span;
            let call = self.parse_expr();
            self.finish_stmt_line("expected a newline after `defer`");
            return Stmt::Defer(call, span);
        }

        // RFC 0046 `unsafe:` block: the same block shape as `if`/`while`
        // bodies. An `unsafe` NOT followed by `:` is the modifier misuse
        // (GOF3140) — the keyword never prefixes a statement.
        if self.matches(TokenDiscriminant::Unsafe) {
            let span = self.previous().span;
            if self.check(TokenDiscriminant::Colon) {
                self.advance();
                let body = self.parse_block("expected an indented block after `unsafe:`");
                return Stmt::Unsafe(body, span);
            }
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3140",
                    "`unsafe` is a block statement in gof and never part of a signature",
                    "write `unsafe:` followed by an indented block, or remove the modifier",
                    span,
                )
                .with_fix_it("remove the `unsafe` modifier, leaving the valid statement")
                .with_source_path(PathBuf::new()),
            );
            // Recover: parse the statement the modifier was prefixing.
            return self.parse_stmt();
        }

        // RFC 0051 GOF3159: `async:` in statement position (the `unsafe:`
        // shape). There are no async blocks in gof; the fix-it suggests
        // the spawn-and-join equivalent.
        if self.matches(TokenDiscriminant::Async) {
            let span = self.previous().span;
            if self.check(TokenDiscriminant::Colon) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3159",
                        "`async:` blocks do not exist in gof",
                        "v1 concurrency is `go` + `await`: spawn the block's body as a named function and join with `await`",
                        span,
                    )
                    .with_fix_it("move the body into a named function and spawn it with `go f(...)`")
                    .with_source_path(PathBuf::new()),
                );
                // Recover: parse the block so its own diagnostics stay
                // reachable, then discard it.
                self.advance();
                let _ = self.parse_block("expected an indented block after `async:`");
                return Stmt::Expr(Expr::Bool(false, span), span);
            }
            // GOF3160: `async` in any other statement position.
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3160",
                    "`async` is reserved and does not name a statement, expression, or type",
                    "`async` is reserved for a possible future suspension model; v1 concurrency is `go` + `await` / `await_result`",
                    span,
                )
                .with_fix_it("remove `async`, or use `go f(...)` + `await t` for the spawn-and-join intent")
                .with_source_path(PathBuf::new()),
            );
            return Stmt::Expr(Expr::Bool(false, span), span);
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
            // RFC 0050: `mut (a, b) = pair` distributes `mut` over every leaf
            // of a tuple pattern. A `mut` target written with parentheses is
            // parsed as an expression first and reinterpreted as a pattern.
            if self.check(TokenDiscriminant::LParen) {
                let expr = self.parse_expr();
                // RFC 0052: `mut (a, b) if c = pair` — a guard cannot sit in
                // a binding statement; recover past it so the `=` follows.
                self.reject_position_guard();
                self.expect(TokenDiscriminant::Equal, "expected `=` in a binding");
                let pattern = self.binding_pattern_from_expr(expr);
                let value = self.parse_expr();
                self.finish_stmt_line("expected a newline after binding");
                return Stmt::Bind {
                    pattern,
                    mutable: true,
                    ty: None,
                    value,
                    span,
                };
            }
            let name = self.expect_ident("expected an identifier after `mut`");
            self.reject_position_guard();
            let ty = self.parse_optional_type_ref();
            self.expect(TokenDiscriminant::Equal, "expected `=` in a binding");
            let value = self.parse_expr();
            self.finish_stmt_line("expected a newline after binding");
            return Stmt::Bind {
                pattern: binding_leaf_from_name(name, span),
                mutable: true,
                ty,
                value,
                span,
            };
        }

        if self.check_ident_binding_or_assignment() {
            let name = self.expect_ident("expected a binding target");
            let span = self.previous().span;
            self.reject_position_guard();
            let ty = self.parse_optional_type_ref();
            self.expect(
                TokenDiscriminant::Equal,
                "expected `=` in an assignment or binding",
            );
            let value = self.parse_expr();
            self.finish_stmt_line("expected a newline after assignment");
            return if ty.is_some() || name == "_" {
                // RFC 0052: `_ = expr` evaluates the right-hand side for
                // its effects and discards the value — the wildcard binds
                // nothing, replacing the accidental `_` name binding.
                Stmt::Bind {
                    pattern: binding_leaf_from_name(name, span),
                    mutable: false,
                    ty,
                    value,
                    span,
                }
            } else {
                Stmt::Assign { name, value, span }
            };
        }

        // RFC 0050: statements may now begin with `(`. The parenthesized form
        // is parsed as an expression; when `=` follows, the tuple literal is
        // reinterpreted as a destructuring pattern whose leaves must be
        // identifiers or nested tuple literals. Both forms were parse errors
        // before, so no existing program changes meaning.
        if self.check(TokenDiscriminant::LParen) {
            let expr = self.parse_expr();
            // RFC 0052: `(a, b) if c = pair` — a guard cannot sit in a
            // binding statement. Report the placement error, recover past
            // the guard expression, and let the binding grammar continue.
            self.reject_position_guard();
            if self.check(TokenDiscriminant::Equal) {
                let span = expr.span();
                self.advance();
                let pattern = self.binding_pattern_from_expr(expr);
                let value = self.parse_expr();
                self.finish_stmt_line("expected a newline after binding");
                return Stmt::Bind {
                    pattern,
                    mutable: false,
                    ty: None,
                    value,
                    span,
                };
            }
            let span = expr.span();
            self.finish_stmt_line("expected a newline after expression");
            return Stmt::Expr(expr, span);
        }

        let expr = self.parse_expr();
        let span = expr.span();
        self.finish_stmt_line("expected a newline after expression");
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
        let mut span = pattern.span();
        // RFC 0052 guards: `pattern if <expr>:` — match arms only. The
        // guard expression extends to the arm's terminating `:`
        // (expression parsing is bracket-aware, so a `:` inside a dict
        // literal in the guard is consumed by dict parsing).
        let guard = if self.check(TokenDiscriminant::If) {
            self.advance();
            let guard = self.parse_expr();
            span = Span::new(span.line, span.column, guard.span().end_column);
            Some(guard)
        } else {
            None
        };
        self.expect(
            TokenDiscriminant::Colon,
            "expected `:` after match arm pattern",
        );
        let body = self.parse_block("expected an indented block after match arm");
        MatchArm {
            pattern,
            guard,
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
                // RFC 0050: each payload binding position accepts a plain
                // identifier (today's behavior) or a nested tuple pattern
                // that destructures a tuple-typed payload field in place.
                bindings.push(self.parse_match_binding());
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
                // RFC 0050: postfix `.` followed by a canonical decimal
                // integer literal accesses a tuple member. Non-canonical
                // literals (leading zeros like `01`) fall through to the
                // existing field path.
                if let Some(index) = self.peek_tuple_index() {
                    let end = self.advance().span;
                    expr = Expr::TupleIndex {
                        target: Box::new(expr),
                        index,
                        span: Span::new(start.line, start.column, end.end_column),
                    };
                    continue;
                }
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
            // RFC 0051 GOF3160: `async` in expression position. The
            // reserved keyword never forms an expression operand.
            TokenKind::Async => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3160",
                        "`async` is reserved and does not name an expression, operand, or type",
                        "`async` is reserved for a possible future suspension model; v1 concurrency is `go` + `await` / `await_result`",
                        token.span,
                    )
                    .with_fix_it("remove `async`, or use `go f(...)` + `await t` for the spawn-and-join intent")
                    .with_source_path(PathBuf::new()),
                );
                Expr::Ident("_error".to_string(), token.span)
            }
            TokenKind::Ident(value) => Expr::Ident(value, token.span),
            TokenKind::LParen => {
                // RFC 0050: the comma decides. `()` and `(x,)` are the
                // degenerate shapes only the parser can see, so it emits
                // GOF3155 directly; one item without a comma stays today's
                // grouping; two or more comma-separated items form a tuple.
                if self.check(TokenDiscriminant::RParen) {
                    let end = self.advance().span;
                    let span = Span::new(token.span.line, token.span.column, end.end_column);
                    self.diagnostics.push(
                        Diagnostic::error(
                            "GOF3155",
                            "tuples require two or more members",
                            "gof has no empty tuple; the unit type is written `unit`",
                            span,
                        )
                        .with_fix_it("replace `()` with `unit`, or add two or more members"),
                    );
                    return Expr::Ident("_error".to_string(), span);
                }
                let first = self.parse_expr();
                if !self.matches(TokenDiscriminant::Comma) {
                    self.expect(
                        TokenDiscriminant::RParen,
                        "expected `)` to close grouped expression",
                    );
                    return first;
                }
                if self.check(TokenDiscriminant::RParen) {
                    let end = self.advance().span;
                    let span = Span::new(token.span.line, token.span.column, end.end_column);
                    self.diagnostics.push(
                        Diagnostic::error(
                            "GOF3155",
                            "tuples require two or more members",
                            "tuples have no single-member form and reject trailing commas",
                            span,
                        )
                        .with_fix_it("remove the trailing comma, or add a second member"),
                    );
                    return Expr::Ident("_error".to_string(), span);
                }
                let mut items = vec![first];
                loop {
                    items.push(self.parse_expr());
                    if !self.matches(TokenDiscriminant::Comma) {
                        break;
                    }
                    if self.check(TokenDiscriminant::RParen) {
                        let end = self.advance().span;
                        let span = Span::new(token.span.line, token.span.column, end.end_column);
                        self.diagnostics.push(
                            Diagnostic::error(
                                "GOF3155",
                                "tuple literals do not allow trailing commas",
                                "the parser requires an expression after every comma, as with list and dict literals",
                                span,
                            )
                            .with_fix_it("remove the trailing comma"),
                        );
                        return Expr::Tuple { items, span };
                    }
                }
                let end = self.expect(
                    TokenDiscriminant::RParen,
                    "expected `)` to close the tuple literal",
                );
                Expr::Tuple {
                    items,
                    span: Span::new(token.span.line, token.span.column, end.end_column),
                }
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
            // RFC 0047 anonymous function expression: `fn` followed by `(`.
            // `fn` followed by an identifier is the named-declaration form,
            // which is module-top-level only — it falls through to the
            // unexpected-token error below exactly as before.
            TokenKind::Fn if self.check(TokenDiscriminant::LParen) => {
                self.advance();
                let params = self.parse_closure_params();
                self.expect(
                    TokenDiscriminant::RParen,
                    "expected `)` after closure parameters",
                );
                let return_type = self.parse_optional_return_type_ref();
                self.expect(
                    TokenDiscriminant::Colon,
                    "expected `:` before the closure body",
                );
                let body =
                    self.parse_block("expected an indented block after the closure signature");
                // The block's Dedent ends the expression; the line hosting
                // the Dedent continues the enclosing expression or statement.
                self.expr_ended_with_block = true;
                let end = self.previous().span;
                Expr::Closure {
                    params,
                    return_type,
                    body,
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
        // RFC 0047 closure types: `fn(ParamType, ...) -> RetType`. The
        // return type is mandatory — `fn` is a keyword, so it can never
        // collide with a user type name, and the form is purely additive.
        if self.check(TokenDiscriminant::Fn) {
            return self.parse_function_type_ref();
        }
        // RFC 0050: `(T1, T2, ...)` with two or more members is a tuple type.
        // The type grammar has no grouping, so a single item in parentheses
        // is the attempted 1-tuple shape and gets GOF3155.
        if self.check(TokenDiscriminant::LParen) {
            return self.parse_tuple_type_ref();
        }
        let name = self.expect_ident(message);
        if name == "tuple" {
            let span = self.previous().span;
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuple types are written (T1, T2)",
                    "the `tuple` name is not a type on its own; a tuple type is its member types in parentheses",
                    span,
                )
                .with_fix_it("replace `tuple` with a tuple type like `(int, string)`"),
            );
        }
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

    /// RFC 0047 function type: `fn(ParamType, ...) -> RetType`. Stored as
    /// `TypeRef { name: "fn", args: [param types..., return type] }` — the
    /// last argument is the return type; the parser guarantees at least
    /// one argument (the mandatory return annotation).
    fn parse_function_type_ref(&mut self) -> TypeRef {
        let start = self.advance().span;
        self.expect(
            TokenDiscriminant::LParen,
            "expected `(` after `fn` in a function type",
        );
        let mut args = Vec::new();
        if !self.check(TokenDiscriminant::RParen) {
            loop {
                args.push(self.parse_type_ref("expected a parameter type"));
                if !self.matches(TokenDiscriminant::Comma) {
                    break;
                }
            }
        }
        self.expect(
            TokenDiscriminant::RParen,
            "expected `)` after function type parameters",
        );
        self.expect(
            TokenDiscriminant::Arrow,
            "expected `->` and a return type to complete the function type",
        );
        let return_type = self.parse_type_ref("expected a return type after `->`");
        let end = return_type.span;
        args.push(return_type);
        TypeRef {
            name: "fn".to_string(),
            args,
            span: Span::new(start.line, start.column, end.end_column),
        }
    }

    fn parse_tuple_type_ref(&mut self) -> TypeRef {
        let start = self.advance().span;
        if self.check(TokenDiscriminant::RParen) {
            let end = self.advance().span;
            let span = Span::new(start.line, start.column, end.end_column);
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuple types require two or more members",
                    "gof has no empty tuple type; the unit type is written `unit`",
                    span,
                )
                .with_fix_it("replace `()` with `unit`, or add two or more member types"),
            );
            return TypeRef {
                name: "tuple".to_string(),
                args: Vec::new(),
                span,
            };
        }
        let first = self.parse_type_ref("expected a type inside the tuple type");
        if !self.matches(TokenDiscriminant::Comma) {
            let end = self.expect(
                TokenDiscriminant::RParen,
                "expected `)` to close the tuple type",
            );
            let span = Span::new(start.line, start.column, end.end_column);
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuple types require two or more members",
                    "the type grammar has no grouping; write the type directly without parentheses",
                    span,
                )
                .with_fix_it("remove the parentheses, or add a second member type"),
            );
            return TypeRef {
                name: "tuple".to_string(),
                args: vec![first],
                span,
            };
        }
        if self.check(TokenDiscriminant::RParen) {
            let end = self.advance().span;
            let span = Span::new(start.line, start.column, end.end_column);
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuple types require two or more members",
                    "tuple types have no single-member form and reject trailing commas",
                    span,
                )
                .with_fix_it("remove the trailing comma, or add a second member type"),
            );
            return TypeRef {
                name: "tuple".to_string(),
                args: vec![first],
                span,
            };
        }
        let mut args = vec![first];
        loop {
            args.push(self.parse_type_ref("expected a type inside the tuple type"));
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
            if self.check(TokenDiscriminant::RParen) {
                let end = self.advance().span;
                let span = Span::new(start.line, start.column, end.end_column);
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3155",
                        "tuple types do not allow trailing commas",
                        "the parser requires a type after every comma, as with list and dict types",
                        span,
                    )
                    .with_fix_it("remove the trailing comma"),
                );
                return TypeRef {
                    name: "tuple".to_string(),
                    args,
                    span,
                };
            }
        }
        let end = self.expect(
            TokenDiscriminant::RParen,
            "expected `)` to close the tuple type",
        );
        TypeRef {
            name: "tuple".to_string(),
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

    /// Terminate a statement whose value is an expression. Normally the
    /// statement ends at its trailing newline; when the expression ended
    /// with a closure block (RFC 0047), the newline was already consumed
    /// before the block's Indent, so the statement ends at the Dedent and
    /// the next line's content continues directly at statement level.
    fn finish_stmt_line(&mut self, message: &'static str) -> Span {
        if self.expr_ended_with_block && !self.check(TokenDiscriminant::Newline) {
            self.expr_ended_with_block = false;
            return self.previous().span;
        }
        self.expr_ended_with_block = false;
        self.expect(TokenDiscriminant::Newline, message)
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

    /// RFC 0052: guards are match-arm-only. When `if` follows a
    /// binding-statement target or a `for` loop binding, report the
    /// specific GOF2002 placement error and recover by consuming the
    /// guard expression so parsing continues at the `=` / `in` the
    /// statement grammar requires.
    fn reject_position_guard(&mut self) {
        if self.check(TokenDiscriminant::If) {
            let span = self.advance().span;
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF2002",
                    "guards apply only to match arms",
                    "binding statements and `for` bindings cannot carry a guard",
                    span,
                )
                .with_fix_it("remove the `if` condition, or rewrite this as a `match` arm"),
            );
            let _ = self.parse_expr();
        }
    }

    /// Save the current token position so a speculative parse can be undone.
    fn mark(&self) -> usize {
        self.current
    }

    /// Restore a token position saved by [`Parser::mark`]. Callers that also
    /// need to undo speculative diagnostics must truncate them separately.
    fn reset(&mut self, mark: usize) {
        self.current = mark;
    }

    /// Peek at a tuple element index: a postfix `.` followed by a canonical
    /// decimal integer literal (no leading zeros, no sign). The lexer keeps
    /// no raw text, so canonicity is checked by comparing the literal's
    /// source width with the value's own digit count (`01` is width 2 for
    /// the value 1). Non-canonical literals return `None` so the caller
    /// falls through to the field-access path.
    fn peek_tuple_index(&self) -> Option<usize> {
        let token = self.peek();
        let TokenKind::IntLiteral(value) = token.kind else {
            return None;
        };
        if value < 0 {
            return None;
        }
        let source_width = token.span.end_column.saturating_sub(token.span.column);
        if source_width == 0 || value.to_string().len() != source_width {
            return None;
        }
        Some(value as usize)
    }

    /// Parse one pattern leaf literal (RFC 0052): an int literal with an
    /// optional leading `-` (a token pair, not an evaluated expression),
    /// a string literal with the existing lexer escapes, or `true` /
    /// `false`. Returns `None` when the next token cannot begin a
    /// literal leaf so the caller falls through to the identifier path.
    fn parse_pattern_leaf_literal(&mut self) -> Option<BindingPattern> {
        if matches!(self.peek().kind, TokenKind::Minus)
            && matches!(
                self.peek_next().map(|token| &token.kind),
                Some(TokenKind::IntLiteral(_))
            )
        {
            let start = self.advance().span;
            let TokenKind::IntLiteral(value) = self.advance().kind else {
                return None;
            };
            return Some(BindingPattern::Literal(
                PatternLiteral::Int(value.wrapping_neg()),
                Span::new(start.line, start.column, self.previous().span.end_column),
            ));
        }
        let literal = match self.peek().kind.clone() {
            TokenKind::IntLiteral(value) => PatternLiteral::Int(value),
            TokenKind::StringLiteral(value) => PatternLiteral::String(value),
            TokenKind::True => PatternLiteral::Bool(true),
            TokenKind::False => PatternLiteral::Bool(false),
            _ => return None,
        };
        let span = self.advance().span;
        Some(BindingPattern::Literal(literal, span))
    }

    /// Parse one binding position in a match arm payload (RFC 0050 +
    /// RFC 0052): a plain identifier, the `_` wildcard, a literal leaf,
    /// or a nested tuple pattern.
    fn parse_match_binding(&mut self) -> BindingPattern {
        if self.check(TokenDiscriminant::LParen) {
            return self.parse_binding_pattern();
        }
        if let Some(literal) = self.parse_pattern_leaf_literal() {
            return literal;
        }
        let name = self.expect_ident("expected a binding name in the payload match pattern");
        let span = self.previous().span;
        binding_leaf_from_name(name, span)
    }

    /// Parse a binding pattern: an identifier leaf, the `_` wildcard, a
    /// literal leaf, or a parenthesized tuple pattern with two or more
    /// members. The degenerate shapes (`()`, `(x)`, trailing commas) are
    /// parser-visible and emit GOF3155 directly.
    fn parse_binding_pattern(&mut self) -> BindingPattern {
        if !self.check(TokenDiscriminant::LParen) {
            if let Some(literal) = self.parse_pattern_leaf_literal() {
                return literal;
            }
            let name = self.expect_ident("expected a binding name");
            let span = self.previous().span;
            return binding_leaf_from_name(name, span);
        }
        let start = self.advance().span;
        if self.check(TokenDiscriminant::RParen) {
            let end = self.advance().span;
            let span = Span::new(start.line, start.column, end.end_column);
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuples require two or more members",
                    "gof has no empty tuple pattern; bind a single name instead",
                    span,
                )
                .with_fix_it("replace `()` with a binding name, or add two or more binding names"),
            );
            return BindingPattern::Tuple(Vec::new(), span);
        }
        let mut leaves = vec![self.parse_binding_pattern()];
        if !self.matches(TokenDiscriminant::Comma) {
            if self.check(TokenDiscriminant::RParen) {
                let end = self.advance().span;
                let span = Span::new(start.line, start.column, end.end_column);
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3155",
                        "tuples require two or more members",
                        "the pattern grammar has no grouping; a single name needs no parentheses",
                        span,
                    )
                    .with_fix_it("remove the parentheses, or add a second binding name"),
                );
                return BindingPattern::Tuple(leaves, span);
            }
            let end = self.expect(
                TokenDiscriminant::RParen,
                "expected `)` to close the tuple pattern",
            );
            return BindingPattern::Tuple(
                leaves,
                Span::new(start.line, start.column, end.end_column),
            );
        }
        if self.check(TokenDiscriminant::RParen) {
            let end = self.advance().span;
            let span = Span::new(start.line, start.column, end.end_column);
            self.diagnostics.push(
                Diagnostic::error(
                    "GOF3155",
                    "tuples require two or more members",
                    "tuple patterns have no single-member form and reject trailing commas",
                    span,
                )
                .with_fix_it("remove the trailing comma, or add a second binding name"),
            );
            return BindingPattern::Tuple(leaves, span);
        }
        loop {
            leaves.push(self.parse_binding_pattern());
            if !self.matches(TokenDiscriminant::Comma) {
                break;
            }
            if self.check(TokenDiscriminant::RParen) {
                let end = self.advance().span;
                let span = Span::new(start.line, start.column, end.end_column);
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3155",
                        "tuple patterns do not allow trailing commas",
                        "the parser requires a binding name after every comma",
                        span,
                    )
                    .with_fix_it("remove the trailing comma"),
                );
                return BindingPattern::Tuple(leaves, span);
            }
        }
        let end = self.expect(
            TokenDiscriminant::RParen,
            "expected `)` to close the tuple pattern",
        );
        BindingPattern::Tuple(leaves, Span::new(start.line, start.column, end.end_column))
    }

    /// Reinterpret a parsed tuple literal as a destructuring pattern. The
    /// statement began with `(` and is followed by `=`, so the expression is
    /// a tuple literal (`(a, b)`) whose leaves must be identifiers, `_`,
    /// literals, or nested tuple literals. A bare identifier can only come
    /// from grouping parentheses like `(x)` — the single-member target
    /// shape — and is rejected with GOF3155.
    fn binding_pattern_from_expr(&mut self, expr: Expr) -> BindingPattern {
        match expr {
            Expr::Tuple { items, span } => {
                let leaves = items
                    .into_iter()
                    .map(|item| self.pattern_leaf_from_expr(item))
                    .collect();
                BindingPattern::Tuple(leaves, span)
            }
            // A bare identifier at the TOP level means the user wrote
            // `(a) = ...`: the expression parser collapses the grouping,
            // so the pattern grammar's no-grouping rule fires here.
            Expr::Ident(name, span) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF3155",
                        "tuples require two or more members",
                        "the target grammar has no grouping; a single name needs no parentheses",
                        span,
                    )
                    .with_fix_it("remove the parentheses around the binding name"),
                );
                binding_leaf_from_name(name, span)
            }
            other => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2001",
                        "pattern leaves must be identifiers, literals, or nested tuple patterns",
                        "tuple destructuring targets must be binding names or literal values, not arbitrary expressions",
                        other.span(),
                    )
                    .with_fix_it("replace this expression with a binding name, a literal, or a nested tuple pattern"),
                );
                BindingPattern::Name("_error".to_string(), other.span())
            }
        }
    }

    /// A leaf INSIDE a tuple pattern: an identifier or `_` is the good case
    /// (silent); literals (including `-1`, which the expression parser
    /// produced as a unary negation) recurse into `Literal` leaves; nested
    /// tuples recurse; anything else is GOF2001. Literal leaves are parsed
    /// here so the typed layer can reject refutable patterns in binding
    /// statements and `for` bindings with GOF3163 (RFC 0052).
    fn pattern_leaf_from_expr(&mut self, expr: Expr) -> BindingPattern {
        match expr {
            Expr::Tuple { items, span } => {
                let leaves = items
                    .into_iter()
                    .map(|item| self.pattern_leaf_from_expr(item))
                    .collect();
                BindingPattern::Tuple(leaves, span)
            }
            Expr::Ident(name, span) => binding_leaf_from_name(name, span),
            Expr::Int(value, span) => BindingPattern::Literal(PatternLiteral::Int(value), span),
            Expr::String(value, span) => {
                BindingPattern::Literal(PatternLiteral::String(value), span)
            }
            Expr::Bool(value, span) => BindingPattern::Literal(PatternLiteral::Bool(value), span),
            Expr::Unary {
                op: UnaryOp::Neg,
                value,
                span,
            } if matches!(value.as_ref(), Expr::Int(..)) => {
                // A negative int literal in a tuple pattern: `(-1, x)`.
                // The expression parser folds `Minus` + `Int` into a unary
                // negation; the pattern grammar treats it as one literal
                // leaf (RFC 0052 lexer note).
                let Expr::Int(value, _) = *value else {
                    unreachable!("guarded by the matches! above");
                };
                BindingPattern::Literal(PatternLiteral::Int(-value), span)
            }
            other => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "GOF2001",
                        "pattern leaves must be identifiers, literals, or nested tuple patterns",
                        "tuple destructuring targets must be binding names or literal values, not arbitrary expressions",
                        other.span(),
                    )
                    .with_fix_it("replace this expression with a binding name, a literal, or a nested tuple pattern"),
                );
                BindingPattern::Name("_error".to_string(), other.span())
            }
        }
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

    /// RFC 0053: does the token AFTER the current one match? Used to
    /// distinguish the `module:` export block from the `module` fixture
    /// scope keyword without consuming.
    fn peek_ahead_is(&self, expected: TokenDiscriminant) -> bool {
        self.peek_next()
            .map(|token| expected.matches(&token.kind))
            .unwrap_or(false)
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
            | Expr::Binary { span, .. }
            | Expr::Tuple { span, .. }
            | Expr::TupleIndex { span, .. }
            | Expr::Closure { span, .. } => *span,
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

/// Build one identifier-position pattern leaf (RFC 0052): `_` is the
/// wildcard — it matches anything and binds nothing — while every other
/// identifier binds. `_` stays a valid identifier everywhere outside
/// pattern positions; wildcard-ness is a property of pattern position,
/// not a new reserved word.
fn binding_leaf_from_name(name: String, span: Span) -> BindingPattern {
    if name == "_" {
        BindingPattern::Wildcard(span)
    } else {
        BindingPattern::Name(name, span)
    }
}

/// Normalized arm-head skeleton for the RFC 0052 duplicate and
/// unreachability rules. Bindings and wildcards are interchangeable
/// slots; literals compare by kind and value; tuples compare pairwise.
///
/// These shape rules are syntactic — no type information is involved —
/// so they live next to the pattern AST and are shared by the typed
/// layer (the check) and the bootstrap evaluator (the backstop), which
/// keeps the two from drifting apart.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchArmSkeleton {
    Slot,
    Literal(PatternLiteral),
    Tuple(Vec<MatchArmSkeleton>),
}

/// The shape facts one match arm contributes to the RFC 0052 arm-set
/// rules: which enum and variant it matches, whether it is guarded, its
/// payload skeleton, and whether its payload is irrefutable at every
/// position (bindings, wildcards, tuples of those).
#[derive(Debug, Clone)]
pub struct MatchArmShape {
    pub enum_name: String,
    pub variant: String,
    pub guarded: bool,
    pub skeleton: Vec<MatchArmSkeleton>,
    pub payload_irrefutable: bool,
}

pub fn match_arm_skeleton_leaf(pattern: &BindingPattern) -> MatchArmSkeleton {
    match pattern {
        BindingPattern::Name(..) | BindingPattern::Wildcard(..) => MatchArmSkeleton::Slot,
        BindingPattern::Literal(literal, _) => MatchArmSkeleton::Literal(literal.clone()),
        BindingPattern::Tuple(leaves, _) => {
            MatchArmSkeleton::Tuple(leaves.iter().map(match_arm_skeleton_leaf).collect())
        }
    }
}

pub fn pattern_is_irrefutable(pattern: &BindingPattern) -> bool {
    match pattern {
        BindingPattern::Name(..) | BindingPattern::Wildcard(..) => true,
        BindingPattern::Literal(..) => false,
        BindingPattern::Tuple(leaves, _) => leaves.iter().all(pattern_is_irrefutable),
    }
}

fn skeleton_contains_literal(skeleton: &[MatchArmSkeleton]) -> bool {
    skeleton.iter().any(|leaf| match leaf {
        MatchArmSkeleton::Slot => false,
        MatchArmSkeleton::Literal(_) => true,
        MatchArmSkeleton::Tuple(nested) => skeleton_contains_literal(nested),
    })
}

/// GOF3030 / GOF3166: two UNGUARDED arms for the same variant whose
/// skeletons are equal (same variant, same literal values in
/// corresponding positions, bindings and wildcards interchangeable as
/// slots). Pure-slot skeletons keep the classic `GOF3030` trigger so
/// every program that compiles today reports the same code; skeletons
/// containing literals are the `GOF3166` family. Arms differing only by
/// a guard are never duplicates — guards never count.
pub fn check_duplicate_arm_skeleton(
    current: &MatchArmShape,
    prior: &[MatchArmShape],
    arm_span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &std::path::Path,
) {
    if current.guarded {
        return;
    }
    let is_duplicate = prior.iter().any(|shape| {
        !shape.guarded && shape.variant == current.variant && shape.skeleton == current.skeleton
    });
    if !is_duplicate {
        return;
    }
    if skeleton_contains_literal(&current.skeleton) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3166",
                "duplicate match arm pattern",
                "two unguarded arms share the same literal payload skeleton",
                arm_span,
            )
            .with_fix_it("remove the duplicate arm, or distinguish it with a guard")
            .with_source_path(source_path.to_path_buf()),
        );
    } else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3030",
                format!(
                    "duplicate match arm for `{}.{}`",
                    current.enum_name, current.variant
                ),
                "each variant can appear only once in a match over the same value space",
                arm_span,
            )
            .with_fix_it("remove the duplicate arm or replace it with another enum variant")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

/// GOF3167: an arm that follows an unguarded arm for the same variant
/// whose payload is irrefutable (bindings, wildcards, tuples of those)
/// can never run — first match wins, and the earlier arm already
/// matches every value of the variant. Exact skeleton duplicates are
/// claimed by GOF3030/GOF3166 first, so the refinement idiom (a guarded
/// arm, then an unguarded total arm) stays legal.
pub fn check_unreachable_arm(
    current: &MatchArmShape,
    prior: &[MatchArmShape],
    arm_span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &std::path::Path,
) {
    for shape in prior {
        if !shape.guarded && shape.variant == current.variant && shape.payload_irrefutable {
            let exact_duplicate = !current.guarded && shape.skeleton == current.skeleton;
            if !exact_duplicate {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3167",
                        "unreachable match arm",
                        format!(
                            "an earlier unguarded `{}.{}` arm already matches every value this arm could match",
                            shape.enum_name, shape.variant
                        ),
                        arm_span,
                    )
                    .with_fix_it("remove the unreachable arm, or move it above the total arm")
                    .with_source_path(source_path.to_path_buf()),
                );
            }
            return;
        }
    }
}

/// RFC 0052 GOF3163: a refutable pattern leaf (a literal) used as a
/// binding-statement target or `for` loop binding. Those positions are
/// not branches — a pattern that can fail to match there has no failing
/// case to handle. Shared by the typed check and the evaluator's
/// backstop so the wording cannot drift.
pub fn report_refutable_pattern(
    position: &str,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &std::path::Path,
) {
    diagnostics.push(
        Diagnostic::error(
            "GOF3163",
            format!("refutable pattern in a {position}"),
            "this pattern can fail to match; use a `match` statement to handle the failing case",
            span,
        )
        .with_source_path(source_path.to_path_buf()),
    );
}

#[derive(Debug, Clone, Copy)]
enum TokenDiscriminant {
    Module,
    Import,
    Struct,
    Enum,
    Protocol,
    Impl,
    Const,
    Unsafe,
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
    Defer,
    Match,
    Select,
    Default,
    Go,
    Async,
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
                | (Self::Protocol, TokenKind::Protocol)
                | (Self::Impl, TokenKind::Impl)
                | (Self::Const, TokenKind::Const)
                | (Self::Unsafe, TokenKind::Unsafe)
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
                | (Self::Defer, TokenKind::Defer)
                | (Self::Match, TokenKind::Match)
                | (Self::Select, TokenKind::Select)
                | (Self::Default, TokenKind::Default)
                | (Self::Go, TokenKind::Go)
                | (Self::Async, TokenKind::Async)
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
            Self::Protocol => "`protocol`",
            Self::Impl => "`impl`",
            Self::Const => "`const`",
            Self::Unsafe => "`unsafe`",
            Self::Fixture => "`fixture`",
            Self::Fn => "`fn`",
            Self::If => "`if`",
            Self::Else => "`else`",
            Self::While => "`while`",
            Self::For => "`for`",
            Self::In => "`in`",
            Self::Break => "`break`",
            Self::Continue => "`continue`",
            Self::Defer => "`defer`",
            Self::Match => "`match`",
            Self::Select => "`select`",
            Self::Default => "`default`",
            Self::Go => "`go`",
            Self::Async => "`async`",
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
    use super::{
        BinaryOp, BindingPattern, Expr, FunctionKind, MatchArmSkeleton, MatchPattern,
        PatternLiteral, SelectArmKind, Stmt, UnaryOp, match_arm_skeleton_leaf, parse,
        pattern_is_irrefutable,
    };
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;
    use crate::source::Span;

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
            Stmt::For {
                pattern: BindingPattern::Name(name, _),
                ..
            } if name == "value"
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
                    } if enum_name == "JobState"
                        && variant == "Running"
                        && bindings.len() == 1
                        && matches!(&bindings[0], BindingPattern::Name(name, _) if name == "pid")
                )
                && matches!(
                    &arms[2].pattern,
                    MatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "JobState"
                        && variant == "Failed"
                        && bindings.len() == 1
                        && matches!(&bindings[0], BindingPattern::Name(name, _) if name == "message")
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

    fn parse_body(source_text: &str) -> Vec<Stmt> {
        let source = SourceFile::new("test.gof", source_text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        module
            .functions
            .into_iter()
            .next()
            .expect("source should contain a function")
            .body
    }

    fn parse_diagnostics(source_text: &str) -> Vec<&'static str> {
        let source = SourceFile::new("test.gof", source_text);
        let tokens = lex(&source).expect("lexing should succeed");
        parse(&CstModule::new(tokens))
            .expect_err("parsing should fail")
            .codes()
    }

    #[test]
    fn parses_tuple_literals_and_preserves_grouping() {
        let body = parse_body(
            "fn main():\n    pair = (1, 2)\n    nested = ((1, 2), (3, 4))\n    grouped = (1 + 2) * 3\n    single = (pair)\n",
        );
        assert!(matches!(
            &body[0],
            Stmt::Assign {
                value: Expr::Tuple { items, .. },
                ..
            } if items.len() == 2
        ));
        assert!(matches!(
            &body[1],
            Stmt::Assign {
                value: Expr::Tuple { items, .. },
                ..
            } if items.len() == 2
                && matches!(&items[0], Expr::Tuple { .. })
                && matches!(&items[1], Expr::Tuple { .. })
        ));
        // `(x)` stays grouping: `(1 + 2) * 3` is a binary expression.
        assert!(matches!(
            &body[2],
            Stmt::Assign {
                value: Expr::Binary { .. },
                ..
            }
        ));
        assert!(matches!(
            &body[3],
            Stmt::Assign {
                value: Expr::Ident(name, _),
                ..
            } if name == "pair"
        ));
    }

    #[test]
    fn tuple_degenerate_shapes_report_gof3155() {
        assert_eq!(
            parse_diagnostics("fn main():\n    empty = ()\n"),
            vec!["GOF3155"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    single = (1,)\n"),
            vec!["GOF3155"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    trailing = (1, 2,)\n"),
            vec!["GOF3155"]
        );
    }

    #[test]
    fn parses_tuple_element_access() {
        let body = parse_body("fn main() -> int:\n    pair = (1, 2)\n    return pair.0 + pair.1\n");
        assert!(matches!(
            &body[1],
            Stmt::Return(
                Expr::Binary {
                    lhs,
                    rhs,
                    ..
                },
                _
            ) if matches!(
                lhs.as_ref(),
                Expr::TupleIndex { index: 0, .. }
            ) && matches!(rhs.as_ref(), Expr::TupleIndex { index: 1, .. })
        ));
    }

    #[test]
    fn non_canonical_tuple_index_falls_through_to_field_path() {
        // `pair.01` is not a canonical decimal literal, so it is rejected by
        // the existing member-name path rather than read as index 1.
        assert_eq!(
            parse_diagnostics("fn main() -> int:\n    pair = (1, 2)\n    return pair.01\n"),
            vec!["GOF2002"]
        );
    }

    #[test]
    fn parses_tuple_type_refs() {
        let body = parse_body(
            "fn main(point: (int, int)) -> list[(int, string)]:\n    x: ((int, int), string) = point\n    return []\n",
        );
        // The function-level annotations are checked through parse_body's
        // module; here the local annotation proves nesting works.
        assert!(matches!(
            &body[0],
            Stmt::Bind { ty: Some(ty), .. } if ty.name == "tuple"
                && ty.args.len() == 2
                && ty.args[0].name == "tuple"
                && ty.args[0].args.len() == 2
                && ty.args[1].name == "string"
        ));
    }

    #[test]
    fn tuple_type_degenerate_shapes_report_gof3155() {
        assert_eq!(
            parse_diagnostics("fn main(x: (int)):\n    return 0\n"),
            vec!["GOF3155"]
        );
        assert_eq!(
            parse_diagnostics("fn main(x: (int,)):\n    return 0\n"),
            vec!["GOF3155"]
        );
        assert_eq!(
            parse_diagnostics("fn main(x: (int, string,)):\n    return 0\n"),
            vec!["GOF3155"]
        );
    }

    #[test]
    fn bare_tuple_type_name_is_rejected() {
        assert_eq!(
            parse_diagnostics("fn main(x: tuple):\n    return 0\n"),
            vec!["GOF3155"]
        );
    }

    #[test]
    fn parses_tuple_patterns_in_bind_statements() {
        let body = parse_body(
            "fn main():\n    (a, b) = origin\n    mut (w, h) = size\n    (x, (y, z)) = frame\n",
        );
        assert!(matches!(
            &body[0],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                mutable: false,
                ty: None,
                ..
            } if leaves.len() == 2
        ));
        assert!(matches!(
            &body[1],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(..),
                mutable: true,
                ..
            }
        ));
        assert!(matches!(
            &body[2],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if leaves.len() == 2
                && matches!(&leaves[1], BindingPattern::Tuple(inner, _) if inner.len() == 2)
        ));
    }

    #[test]
    fn tuple_pattern_leaves_must_be_identifiers_or_literals() {
        // RFC 0052: literal leaves now PARSE (the typed layer rejects
        // refutable patterns in binding statements with GOF3163).
        let body = parse_body("fn main():\n    (a, 1) = origin\n");
        assert!(matches!(
            &body[0],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if leaves.len() == 2
                && matches!(&leaves[0], BindingPattern::Name(name, _) if name == "a")
                && matches!(
                    &leaves[1],
                    BindingPattern::Literal(crate::ast::PatternLiteral::Int(1), _)
                )
        ));
        // Negative int literals fold into one Literal leaf.
        let body = parse_body("fn main():\n    (a, -1) = origin\n");
        assert!(matches!(
            &body[0],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if matches!(
                &leaves[1],
                BindingPattern::Literal(crate::ast::PatternLiteral::Int(-1), _)
            )
        ));
        // Arbitrary expressions are still malformed leaves (GOF2001).
        assert_eq!(
            parse_diagnostics("fn main():\n    (a, a + b) = origin\n"),
            vec!["GOF2001"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    (a) = origin\n"),
            vec!["GOF3155"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    mut (a + b) = origin\n"),
            vec!["GOF2001"]
        );
    }

    #[test]
    fn parses_tuple_patterns_in_for_loops() {
        let body = parse_body(
            "fn main():\n    total = 0\n    for (w, h) in boxes:\n        total = total + w * h\n    for (a, (b, c)) in triples:\n        total = total + a\n    for value in values:\n        total = total + value\n",
        );
        assert!(matches!(
            &body[1],
            Stmt::For {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if leaves.len() == 2
        ));
        assert!(matches!(
            &body[2],
            Stmt::For {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if leaves.len() == 2
                && matches!(&leaves[1], BindingPattern::Tuple(inner, _) if inner.len() == 2)
        ));
        assert!(matches!(
            &body[3],
            Stmt::For {
                pattern: BindingPattern::Name(name, _),
                ..
            } if name == "value"
        ));
    }

    #[test]
    fn for_tuple_pattern_requires_in_after_the_pattern() {
        // A parenthesized binding that is not followed by `in` rewinds to
        // today's expression grammar; the shared `in` expectation reports
        // the first recoverable error and the parser's normal recovery
        // cascade follows (matching how other malformed statements
        // report).
        let codes = parse_diagnostics("fn main():\n    for (a, b) of boxes:\n        return\n");
        assert_eq!(codes.first().copied(), Some("GOF2002"));
    }

    #[test]
    fn parses_tuple_patterns_in_match_payloads() {
        let source = SourceFile::new(
            "test.gof",
            "enum Gesture:\n    Pan(delta: (int, int))\n    Pinch(start: (int, int), end: (int, int))\n\nfn main(gesture: Gesture) -> bool:\n    match gesture:\n        Gesture.Pan((dx, dy)):\n            return dx == 0\n        Gesture.Pinch((x0, y0), (x1, y1)):\n            return x1 > x0\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let Stmt::Match { arms, .. } = &module.functions[0].body[0] else {
            panic!("expected a match statement");
        };
        let MatchPattern::EnumVariant { bindings, .. } = &arms[0].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Tuple(leaves, _) if leaves.len() == 2
        ));
        let MatchPattern::EnumVariant { bindings, .. } = &arms[1].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(bindings.len() == 2);
        assert!(matches!(
            &bindings[0],
            BindingPattern::Tuple(leaves, _) if leaves.len() == 2
        ));
        assert!(matches!(
            &bindings[1],
            BindingPattern::Tuple(leaves, _) if leaves.len() == 2
        ));
    }

    #[test]
    fn match_payload_single_parenthesized_binding_reports_gof3155() {
        assert_eq!(
            parse_diagnostics(
                "enum Status:\n    Ready\n    Busy\n\nfn main(status: Status) -> bool:\n    match status:\n        Status.Busy((x)):\n            return true\n        Status.Ready:\n            return false\n"
            ),
            vec!["GOF3155"]
        );
    }

    #[test]
    fn parses_match_arm_guards_with_colon_termination() {
        let source = SourceFile::new(
            "test.gof",
            "enum Lamp:\n    On(level: int)\n    Off\n\nfn main(lamp: Lamp) -> int:\n    match lamp:\n        Lamp.On(level) if level > 2:\n            return level\n        Lamp.On(level):\n            return 0\n        Lamp.Off:\n            return 1\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let Stmt::Match { arms, .. } = &module.functions[0].body[0] else {
            panic!("expected a match statement");
        };
        assert!(matches!(
            &arms[0].guard,
            Some(Expr::Binary {
                op: BinaryOp::Gt,
                ..
            })
        ));
        assert!(arms[1].guard.is_none());
        assert!(arms[2].guard.is_none());
        // The guard expression extends to the arm's terminating `:`
        // (bracket-aware parsing), so a dict literal inside a guard ends
        // at its own braces and the arm still terminates at `:`.
        let source = SourceFile::new(
            "test.gof",
            "enum Lamp:\n    On(level: int)\n    Off\n\nfn main(lamp: Lamp, table: dict) -> int:\n    match lamp:\n        Lamp.On(level) if len(table) > 0 and level > 2:\n            return level\n        Lamp.On(level):\n            return 0\n        Lamp.Off:\n            return 1\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let Stmt::Match { arms, .. } = &module.functions[0].body[0] else {
            panic!("expected a match statement");
        };
        assert!(matches!(
            &arms[0].guard,
            Some(Expr::Binary {
                op: BinaryOp::And,
                ..
            })
        ));
    }

    #[test]
    fn guards_are_rejected_outside_match_arms() {
        // RFC 0052: guards are match-arm-only; `if` after a binding
        // statement target or a `for` binding is a specific GOF2002.
        assert_eq!(
            parse_diagnostics("fn main():\n    (a, b) if a > 0 = origin\n"),
            vec!["GOF2002"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    mut (a, b) if a > 0 = origin\n"),
            vec!["GOF2002"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    for (a, b) if a > 0 in pairs:\n        return 0\n"),
            vec!["GOF2002"]
        );
        assert_eq!(
            parse_diagnostics("fn main():\n    for item if true in pairs:\n        return 0\n"),
            vec!["GOF2002"]
        );
    }

    #[test]
    fn parses_literal_and_wildcard_payload_leaves() {
        let source = SourceFile::new(
            "test.gof",
            "enum HttpOutcome:\n    Done(status: int, body: string)\n    Flag(active: bool)\n\nfn main(outcome: HttpOutcome) -> int:\n    match outcome:\n        HttpOutcome.Done(-1, _):\n            return 1\n        HttpOutcome.Done(200, body):\n            return 2\n        HttpOutcome.Done(status, \"plain\"):\n            return 3\n        HttpOutcome.Done(status, body):\n            return 4\n        HttpOutcome.Flag(true):\n            return 5\n        HttpOutcome.Flag(active):\n            return 6\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let Stmt::Match { arms, .. } = &module.functions[0].body[0] else {
            panic!("expected a match statement");
        };
        let MatchPattern::EnumVariant { bindings, .. } = &arms[0].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Literal(PatternLiteral::Int(-1), _)
        ));
        assert!(matches!(&bindings[1], BindingPattern::Wildcard(_)));
        let MatchPattern::EnumVariant { bindings, .. } = &arms[1].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Literal(PatternLiteral::Int(200), _)
        ));
        let MatchPattern::EnumVariant { bindings, .. } = &arms[2].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[1],
            BindingPattern::Literal(PatternLiteral::String(text), _) if text == "plain"
        ));
        let MatchPattern::EnumVariant { bindings, .. } = &arms[4].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Literal(PatternLiteral::Bool(true), _)
        ));
    }

    #[test]
    fn parses_wildcard_leaves_in_bind_and_for_positions() {
        let body = parse_body(
            "fn main():\n    (_, y) = point\n    mut (w, _) = size\n    _ = compute()\n    mut _ = probe()\n    for (i, _) in pairs:\n        print(i)\n    for _ in values:\n        print(1)\n",
        );
        assert!(matches!(
            &body[0],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if matches!(&leaves[0], BindingPattern::Wildcard(_))
                && matches!(&leaves[1], BindingPattern::Name(name, _) if name == "y")
        ));
        assert!(matches!(
            &body[1],
            Stmt::Bind {
                pattern: BindingPattern::Tuple(leaves, _),
                mutable: true,
                ..
            } if matches!(&leaves[1], BindingPattern::Wildcard(_))
        ));
        // `_ = expr` is the explicit evaluate-and-discard idiom, not an
        // assignment to a name `_`.
        assert!(matches!(
            &body[2],
            Stmt::Bind {
                pattern: BindingPattern::Wildcard(_),
                ..
            }
        ));
        assert!(matches!(
            &body[3],
            Stmt::Bind {
                pattern: BindingPattern::Wildcard(_),
                mutable: true,
                ..
            }
        ));
        assert!(matches!(
            &body[4],
            Stmt::For {
                pattern: BindingPattern::Tuple(leaves, _),
                ..
            } if matches!(&leaves[1], BindingPattern::Wildcard(_))
        ));
        assert!(matches!(
            &body[5],
            Stmt::For {
                pattern: BindingPattern::Wildcard(_),
                ..
            }
        ));
    }

    #[test]
    fn parses_nested_literal_in_tuple_payload_patterns() {
        let source = SourceFile::new(
            "test.gof",
            "enum Wrap:\n    Box(pair: (int, string))\n\nfn main(wrap: Wrap) -> int:\n    match wrap:\n        Wrap.Box((404, tag)):\n            return len(tag)\n        Wrap.Box((_, tag)):\n            return len(tag) + 1\n        Wrap.Box(pair):\n            return len(pair.1)\n",
        );
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let Stmt::Match { arms, .. } = &module.functions[0].body[0] else {
            panic!("expected a match statement");
        };
        let MatchPattern::EnumVariant { bindings, .. } = &arms[0].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Tuple(leaves, _)
                if matches!(
                    &leaves[0],
                    BindingPattern::Literal(PatternLiteral::Int(404), _)
                ) && matches!(&leaves[1], BindingPattern::Name(name, _) if name == "tag")
        ));
        let MatchPattern::EnumVariant { bindings, .. } = &arms[1].pattern else {
            panic!("expected an enum variant pattern");
        };
        assert!(matches!(
            &bindings[0],
            BindingPattern::Tuple(leaves, _)
                if matches!(&leaves[0], BindingPattern::Wildcard(_))
        ));
    }

    #[test]
    fn pattern_leaves_bind_no_wildcard_names() {
        // The shared shape helpers: wildcards bind nothing, literals are
        // refutable, skeletons treat slots as interchangeable.
        let leaves = vec![
            BindingPattern::Wildcard(Span::new(1, 1, 2)),
            BindingPattern::Name("x".to_string(), Span::new(1, 4, 5)),
            BindingPattern::Tuple(
                vec![
                    BindingPattern::Literal(PatternLiteral::Int(1), Span::new(1, 7, 8)),
                    BindingPattern::Wildcard(Span::new(1, 10, 11)),
                ],
                Span::new(1, 6, 12),
            ),
        ];
        assert_eq!(
            leaves
                .iter()
                .flat_map(|leaf| leaf.binding_names())
                .collect::<Vec<_>>(),
            vec![("x".to_string(), Span::new(1, 4, 5))]
        );
        assert!(pattern_is_irrefutable(&leaves[0]));
        assert!(pattern_is_irrefutable(&leaves[1]));
        assert!(!pattern_is_irrefutable(&leaves[2]));
        assert_eq!(match_arm_skeleton_leaf(&leaves[0]), MatchArmSkeleton::Slot);
        assert_eq!(match_arm_skeleton_leaf(&leaves[1]), MatchArmSkeleton::Slot);
        assert_eq!(
            match_arm_skeleton_leaf(&leaves[2]),
            MatchArmSkeleton::Tuple(vec![
                MatchArmSkeleton::Literal(PatternLiteral::Int(1)),
                MatchArmSkeleton::Slot,
            ])
        );
    }
}
