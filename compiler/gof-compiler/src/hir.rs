use crate::ast::{
    BinaryOp, EnumDecl, EnumVariant, EnumVariantField, Expr, MatchPattern, Module, Param,
    SelectArm, SelectArmKind, Stmt, StructDecl, StructField, TypeRef, UnaryOp,
};
use crate::source::Span;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct HirModule {
    pub structs: Vec<HirStruct>,
    pub enums: Vec<HirEnum>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirFunction {
    pub id: usize,
    pub receiver_type: Option<HirTypeRef>,
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: Option<HirTypeRef>,
    pub body: Vec<HirStmt>,
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

#[derive(Debug, Clone, Serialize)]
pub struct HirParam {
    pub name: String,
    pub ty: Option<HirTypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirTypeRef {
    pub name: String,
    pub args: Vec<HirTypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirStmt {
    Return(HirExpr, Span),
    Bind {
        name: String,
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
        binding: String,
        iterable: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
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
    pub body: Vec<HirStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirMatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<String>,
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
    Recv { operation: HirExpr },
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
                receiver_type: function.receiver_type.as_ref().map(lower_type_ref),
                name: function.name.clone(),
                params: function.params.iter().map(lower_param).collect(),
                return_type: function.return_type.as_ref().map(lower_type_ref),
                body: function.body.iter().map(lower_stmt).collect(),
                source_path: function.source_path.clone(),
            })
            .collect(),
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
            name,
            mutable,
            ty,
            value,
            span,
        } => HirStmt::Bind {
            name: name.clone(),
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
            binding,
            iterable,
            body,
            span,
        } => HirStmt::For {
            binding: binding.clone(),
            iterable: lower_expr(iterable),
            body: body.iter().map(lower_stmt).collect(),
            span: *span,
        },
        Stmt::Break(span) => HirStmt::Break(*span),
        Stmt::Continue(span) => HirStmt::Continue(*span),
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
            bindings: bindings.clone(),
            span: *span,
        },
    }
}

fn lower_select_arm(arm: &SelectArm) -> HirSelectArm {
    HirSelectArm {
        binding: arm.binding.clone(),
        kind: match &arm.kind {
            SelectArmKind::Recv { operation } => HirSelectArmKind::Recv {
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
            | HirExpr::Binary { span, .. } => *span,
        }
    }
}
