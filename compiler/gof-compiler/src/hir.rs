use crate::ast::{
    BinaryOp, EnumDecl, EnumVariant, EnumVariantField, Expr, FixtureScopeRef, FunctionKind,
    ImplDecl, MatchPattern, Module, Param, ProtocolDecl, ProtocolRequirement, SelectArm,
    SelectArmKind, Stmt, StructDecl, StructField, TypeParam, TypeRef, UnaryOp,
};
use crate::source::Span;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct HirModule {
    pub structs: Vec<HirStruct>,
    pub enums: Vec<HirEnum>,
    pub functions: Vec<HirFunction>,
    /// RFC 0044 protocol declarations, mirroring `ast::ProtocolDecl`.
    pub protocols: Vec<HirProtocolDecl>,
    /// RFC 0044 explicit conformance declarations, mirroring
    /// `ast::ImplDecl`.
    pub impls: Vec<HirImplDecl>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirFunction {
    pub id: usize,
    pub kind: FunctionKind,
    pub fixture_scope: Option<FixtureScopeRef>,
    pub receiver_type: Option<HirTypeRef>,
    pub name: String,
    pub params: Vec<HirParam>,
    /// RFC 0044 protocol-constrained type parameters: `[T: Shape]`.
    pub type_params: Vec<HirTypeParam>,
    pub return_type: Option<HirTypeRef>,
    pub body: Vec<HirStmt>,
    pub source_path: PathBuf,
}

/// RFC 0044 type parameter with its protocol bounds, mirroring
/// `ast::TypeParam`.
#[derive(Debug, Clone, Serialize)]
pub struct HirTypeParam {
    pub name: String,
    pub bounds: Vec<String>,
    pub span: Span,
}

/// RFC 0044 protocol declaration, mirroring `ast::ProtocolDecl`.
#[derive(Debug, Clone, Serialize)]
pub struct HirProtocolDecl {
    pub name: String,
    pub requirements: Vec<HirProtocolRequirement>,
    pub span: Span,
    pub source_path: PathBuf,
}

/// One signature-only requirement. The receiver is the first parameter;
/// a mutating requirement carries `mut` on it (`params[0].mutable`).
#[derive(Debug, Clone, Serialize)]
pub struct HirProtocolRequirement {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: Option<HirTypeRef>,
    pub span: Span,
}

/// RFC 0044 explicit conformance declaration, mirroring
/// `ast::ImplDecl`.
#[derive(Debug, Clone, Serialize)]
pub struct HirImplDecl {
    pub struct_name: String,
    pub protocols: Vec<String>,
    pub span: Span,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirStruct {
    pub name: String,
    pub fields: Vec<HirStructField>,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirStructField {
    pub name: String,
    pub ty: HirTypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirEnum {
    pub name: String,
    pub variants: Vec<HirEnumVariant>,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirEnumVariant {
    pub name: String,
    pub fields: Vec<HirEnumVariantField>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirEnumVariantField {
    pub name: String,
    pub ty: HirTypeRef,
    pub span: Span,
}

impl HirBindingPattern {
    /// Every leaf binding name in this pattern, left to right. Mirrors
    /// `ast::BindingPattern::binding_names`; used by the closure capture
    /// analysis to classify pattern-declared locals.
    pub fn binding_names(&self) -> Vec<(String, Span)> {
        let mut names = Vec::new();
        self.collect_names(&mut names);
        names
    }

    fn collect_names(&self, names: &mut Vec<(String, Span)>) {
        match self {
            HirBindingPattern::Name(name, span) => names.push((name.clone(), *span)),
            HirBindingPattern::Tuple(leafs, _) => {
                for leaf in leafs {
                    leaf.collect_names(names);
                }
            }
            HirBindingPattern::Wildcard(_) | HirBindingPattern::Literal(_, _) => {}
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HirParam {
    pub name: String,
    /// RFC 0044 `mut` marker: `mut self: Counter` receivers and
    /// `mut counter: T` generic parameters.
    pub mutable: bool,
    pub ty: Option<HirTypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirTypeRef {
    pub name: String,
    pub args: Vec<HirTypeRef>,
    pub span: Span,
}

/// A literal value in pattern position (RFC 0052), mirroring
/// `ast::PatternLiteral`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HirPatternLiteral {
    Int(i64),
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize)]
pub enum HirBindingPattern {
    Name(String, Span),
    Tuple(Vec<HirBindingPattern>, Span),
    /// RFC 0052 `_` leaf: matches anything, binds nothing.
    Wildcard(Span),
    /// RFC 0052 literal leaf: an int / string / bool value test.
    Literal(HirPatternLiteral, Span),
}

#[derive(Debug, Clone, Serialize)]
pub enum HirStmt {
    Return(HirExpr, Span),
    Bind {
        pattern: HirBindingPattern,
        mutable: bool,
        ty: Option<HirTypeRef>,
        value: HirExpr,
        span: Span,
    },
    Assign {
        name: String,
        value: HirExpr,
        span: Span,
    },
    If {
        condition: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
        span: Span,
    },
    While {
        condition: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    For {
        pattern: HirBindingPattern,
        iterable: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Defer(HirExpr, Span),
    /// RFC 0046 `unsafe:` block: permission marker, plain block
    /// semantics. Rule 2: every typed-HIR check applies inside exactly
    /// as outside; the evaluator runs it as an ordinary scoped block.
    Unsafe(Vec<HirStmt>, Span),
    Match {
        value: HirExpr,
        arms: Vec<HirMatchArm>,
        span: Span,
    },
    Select {
        arms: Vec<HirSelectArm>,
        span: Span,
    },
    Expr(HirExpr, Span),
}

#[derive(Debug, Clone, Serialize)]
pub struct HirMatchArm {
    pub pattern: HirMatchPattern,
    /// RFC 0052 guard: `pattern if <bool expr>:` — match arms only.
    /// Evaluated after the pattern matched and its bindings were
    /// created; a `false` guard falls through to the next arm.
    pub guard: Option<HirExpr>,
    pub body: Vec<HirStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirMatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<HirBindingPattern>,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct HirSelectArm {
    pub binding: Option<String>,
    pub kind: HirSelectArmKind,
    pub body: Vec<HirStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirSelectArmKind {
    Operation { operation: HirExpr },
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirDictEntry {
    pub key: HirExpr,
    pub value: HirExpr,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirExpr {
    Int(i64, Span),
    String(String, Span),
    Bool(bool, Span),
    Local(String, Span),
    List {
        items: Vec<HirExpr>,
        span: Span,
    },
    Dict {
        entries: Vec<HirDictEntry>,
        span: Span,
    },
    Call {
        callee: String,
        args: Vec<HirExpr>,
        span: Span,
    },
    Field {
        target: Box<HirExpr>,
        field: String,
        span: Span,
    },
    MethodCall {
        target: Box<HirExpr>,
        method: String,
        args: Vec<HirExpr>,
        span: Span,
    },
    Index {
        target: Box<HirExpr>,
        index: Box<HirExpr>,
        span: Span,
    },
    Go {
        value: Box<HirExpr>,
        span: Span,
    },
    Await {
        value: Box<HirExpr>,
        span: Span,
    },
    Propagate {
        value: Box<HirExpr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        value: Box<HirExpr>,
        span: Span,
    },
    Binary {
        lhs: Box<HirExpr>,
        op: BinaryOp,
        rhs: Box<HirExpr>,
        span: Span,
    },
    Tuple {
        items: Vec<HirExpr>,
        span: Span,
    },
    TupleIndex {
        target: Box<HirExpr>,
        index: usize,
        span: Span,
    },
    /// RFC 0047 anonymous function expression, mirroring
    /// `ast::Expr::Closure`.
    Closure {
        params: Vec<HirParam>,
        return_type: Option<HirTypeRef>,
        body: Vec<HirStmt>,
        span: Span,
    },
}

pub fn lower(module: &Module) -> HirModule {
    HirModule {
        structs: module.structs.iter().map(lower_struct).collect(),
        enums: module.enums.iter().map(lower_enum).collect(),
        functions: module
            .functions
            .iter()
            .enumerate()
            .map(|(id, function)| HirFunction {
                id,
                kind: function.kind,
                fixture_scope: function.fixture_scope.clone(),
                receiver_type: function.receiver_type.as_ref().map(lower_type_ref),
                name: function.name.clone(),
                params: function.params.iter().map(lower_param).collect(),
                type_params: function.type_params.iter().map(lower_type_param).collect(),
                return_type: function.return_type.as_ref().map(lower_type_ref),
                body: function.body.iter().map(lower_stmt).collect(),
                source_path: function.source_path.clone(),
            })
            .collect(),
        protocols: module.protocols.iter().map(lower_protocol).collect(),
        impls: module.impls.iter().map(lower_impl).collect(),
    }
}

fn lower_protocol(decl: &ProtocolDecl) -> HirProtocolDecl {
    HirProtocolDecl {
        name: decl.name.clone(),
        requirements: decl
            .requirements
            .iter()
            .map(lower_protocol_requirement)
            .collect(),
        span: decl.span,
        source_path: decl.source_path.clone(),
    }
}

fn lower_protocol_requirement(requirement: &ProtocolRequirement) -> HirProtocolRequirement {
    HirProtocolRequirement {
        name: requirement.name.clone(),
        params: requirement.params.iter().map(lower_param).collect(),
        return_type: requirement.return_type.as_ref().map(lower_type_ref),
        span: requirement.span,
    }
}

fn lower_impl(decl: &ImplDecl) -> HirImplDecl {
    HirImplDecl {
        struct_name: decl.struct_name.clone(),
        protocols: decl.protocols.clone(),
        span: decl.span,
        source_path: decl.source_path.clone(),
    }
}

fn lower_type_param(param: &TypeParam) -> HirTypeParam {
    HirTypeParam {
        name: param.name.clone(),
        bounds: param.bounds.clone(),
        span: param.span,
    }
}

fn lower_struct(decl: &StructDecl) -> HirStruct {
    HirStruct {
        name: decl.name.clone(),
        fields: decl.fields.iter().map(lower_struct_field).collect(),
        source_path: decl.source_path.clone(),
    }
}

fn lower_struct_field(field: &StructField) -> HirStructField {
    HirStructField {
        name: field.name.clone(),
        ty: lower_type_ref(&field.ty),
        span: field.span,
    }
}

fn lower_enum(decl: &EnumDecl) -> HirEnum {
    HirEnum {
        name: decl.name.clone(),
        variants: decl.variants.iter().map(lower_enum_variant).collect(),
        source_path: decl.source_path.clone(),
    }
}

fn lower_enum_variant(variant: &EnumVariant) -> HirEnumVariant {
    HirEnumVariant {
        name: variant.name.clone(),
        fields: variant
            .fields
            .iter()
            .map(lower_enum_variant_field)
            .collect(),
        span: variant.span,
    }
}

fn lower_enum_variant_field(field: &EnumVariantField) -> HirEnumVariantField {
    HirEnumVariantField {
        name: field.name.clone(),
        ty: lower_type_ref(&field.ty),
        span: field.span,
    }
}

fn lower_param(param: &Param) -> HirParam {
    HirParam {
        name: param.name.clone(),
        mutable: param.mutable,
        ty: param.ty.as_ref().map(lower_type_ref),
        span: param.span,
    }
}

fn lower_type_ref(ty: &TypeRef) -> HirTypeRef {
    HirTypeRef {
        name: ty.name.clone(),
        args: ty.args.iter().map(lower_type_ref).collect(),
        span: ty.span,
    }
}

fn lower_stmt(stmt: &Stmt) -> HirStmt {
    match stmt {
        Stmt::Return(expr, span) => HirStmt::Return(lower_expr(expr), *span),
        Stmt::Bind {
            pattern,
            mutable,
            ty,
            value,
            span,
        } => HirStmt::Bind {
            pattern: lower_binding_pattern(pattern),
            mutable: *mutable,
            ty: ty.as_ref().map(lower_type_ref),
            value: lower_expr(value),
            span: *span,
        },
        Stmt::Assign { name, value, span } => HirStmt::Assign {
            name: name.clone(),
            value: lower_expr(value),
            span: *span,
        },
        Stmt::If {
            condition,
            then_body,
            else_body,
            span,
        } => HirStmt::If {
            condition: lower_expr(condition),
            then_body: then_body.iter().map(lower_stmt).collect(),
            else_body: else_body.iter().map(lower_stmt).collect(),
            span: *span,
        },
        Stmt::While {
            condition,
            body,
            span,
        } => HirStmt::While {
            condition: lower_expr(condition),
            body: body.iter().map(lower_stmt).collect(),
            span: *span,
        },
        Stmt::For {
            pattern,
            iterable,
            body,
            span,
        } => HirStmt::For {
            pattern: lower_binding_pattern(pattern),
            iterable: lower_expr(iterable),
            body: body.iter().map(lower_stmt).collect(),
            span: *span,
        },
        Stmt::Break(span) => HirStmt::Break(*span),
        Stmt::Continue(span) => HirStmt::Continue(*span),
        Stmt::Defer(call, span) => HirStmt::Defer(lower_expr(call), *span),
        Stmt::Unsafe(body, span) => HirStmt::Unsafe(body.iter().map(lower_stmt).collect(), *span),
        Stmt::Match { value, arms, span } => HirStmt::Match {
            value: lower_expr(value),
            arms: arms.iter().map(lower_match_arm).collect(),
            span: *span,
        },
        Stmt::Select { arms, span } => HirStmt::Select {
            arms: arms.iter().map(lower_select_arm).collect(),
            span: *span,
        },
        Stmt::Expr(expr, span) => HirStmt::Expr(lower_expr(expr), *span),
    }
}

fn lower_match_arm(arm: &crate::ast::MatchArm) -> HirMatchArm {
    HirMatchArm {
        pattern: lower_match_pattern(&arm.pattern),
        guard: arm.guard.as_ref().map(lower_expr),
        body: arm.body.iter().map(lower_stmt).collect(),
        span: arm.span,
    }
}

fn lower_match_pattern(pattern: &MatchPattern) -> HirMatchPattern {
    match pattern {
        MatchPattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            span,
        } => HirMatchPattern::EnumVariant {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            bindings: bindings.iter().map(lower_binding_pattern).collect(),
            span: *span,
        },
    }
}

fn lower_binding_pattern(pattern: &crate::ast::BindingPattern) -> HirBindingPattern {
    match pattern {
        crate::ast::BindingPattern::Name(name, span) => {
            HirBindingPattern::Name(name.clone(), *span)
        }
        crate::ast::BindingPattern::Tuple(leafs, span) => {
            HirBindingPattern::Tuple(leafs.iter().map(lower_binding_pattern).collect(), *span)
        }
        crate::ast::BindingPattern::Wildcard(span) => HirBindingPattern::Wildcard(*span),
        crate::ast::BindingPattern::Literal(literal, span) => HirBindingPattern::Literal(
            match literal {
                crate::ast::PatternLiteral::Int(value) => HirPatternLiteral::Int(*value),
                crate::ast::PatternLiteral::String(value) => {
                    HirPatternLiteral::String(value.clone())
                }
                crate::ast::PatternLiteral::Bool(value) => HirPatternLiteral::Bool(*value),
            },
            *span,
        ),
    }
}

fn lower_select_arm(arm: &SelectArm) -> HirSelectArm {
    HirSelectArm {
        binding: arm.binding.clone(),
        kind: match &arm.kind {
            SelectArmKind::Operation { operation } => HirSelectArmKind::Operation {
                operation: lower_expr(operation),
            },
            SelectArmKind::Default => HirSelectArmKind::Default,
        },
        body: arm.body.iter().map(lower_stmt).collect(),
        span: arm.span,
    }
}

fn lower_expr(expr: &Expr) -> HirExpr {
    match expr {
        Expr::Int(value, span) => HirExpr::Int(*value, *span),
        Expr::String(value, span) => HirExpr::String(value.clone(), *span),
        Expr::Bool(value, span) => HirExpr::Bool(*value, *span),
        Expr::Ident(value, span) => HirExpr::Local(value.clone(), *span),
        Expr::List { items, span } => HirExpr::List {
            items: items.iter().map(lower_expr).collect(),
            span: *span,
        },
        Expr::Dict { entries, span } => HirExpr::Dict {
            entries: entries
                .iter()
                .map(|entry| HirDictEntry {
                    key: lower_expr(&entry.key),
                    value: lower_expr(&entry.value),
                })
                .collect(),
            span: *span,
        },
        Expr::Call { callee, args, span } => HirExpr::Call {
            callee: callee.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        Expr::Field {
            target,
            field,
            span,
        } => HirExpr::Field {
            target: Box::new(lower_expr(target)),
            field: field.clone(),
            span: *span,
        },
        Expr::MethodCall {
            target,
            method,
            args,
            span,
        } => HirExpr::MethodCall {
            target: Box::new(lower_expr(target)),
            method: method.clone(),
            args: args.iter().map(lower_expr).collect(),
            span: *span,
        },
        Expr::Index {
            target,
            index,
            span,
        } => HirExpr::Index {
            target: Box::new(lower_expr(target)),
            index: Box::new(lower_expr(index)),
            span: *span,
        },
        Expr::Go { value, span } => HirExpr::Go {
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        Expr::Await { value, span } => HirExpr::Await {
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        Expr::Propagate { value, span } => HirExpr::Propagate {
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        Expr::Unary { op, value, span } => HirExpr::Unary {
            op: *op,
            value: Box::new(lower_expr(value)),
            span: *span,
        },
        Expr::Binary { lhs, op, rhs, span } => HirExpr::Binary {
            lhs: Box::new(lower_expr(lhs)),
            op: *op,
            rhs: Box::new(lower_expr(rhs)),
            span: *span,
        },
        Expr::Tuple { items, span } => HirExpr::Tuple {
            items: items.iter().map(lower_expr).collect(),
            span: *span,
        },
        Expr::TupleIndex {
            target,
            index,
            span,
        } => HirExpr::TupleIndex {
            target: Box::new(lower_expr(target)),
            index: *index,
            span: *span,
        },
        Expr::Closure {
            params,
            return_type,
            body,
            span,
        } => HirExpr::Closure {
            params: params.iter().map(lower_param).collect(),
            return_type: return_type.as_ref().map(lower_type_ref),
            body: body.iter().map(lower_stmt).collect(),
            span: *span,
        },
    }
}

impl HirExpr {
    pub fn span(&self) -> Span {
        match self {
            HirExpr::Int(_, span)
            | HirExpr::String(_, span)
            | HirExpr::Bool(_, span)
            | HirExpr::Local(_, span)
            | HirExpr::List { span, .. }
            | HirExpr::Dict { span, .. }
            | HirExpr::Call { span, .. }
            | HirExpr::Field { span, .. }
            | HirExpr::MethodCall { span, .. }
            | HirExpr::Index { span, .. }
            | HirExpr::Go { span, .. }
            | HirExpr::Await { span, .. }
            | HirExpr::Propagate { span, .. }
            | HirExpr::Unary { span, .. }
            | HirExpr::Binary { span, .. }
            | HirExpr::Tuple { span, .. }
            | HirExpr::TupleIndex { span, .. }
            | HirExpr::Closure { span, .. } => *span,
        }
    }
}
