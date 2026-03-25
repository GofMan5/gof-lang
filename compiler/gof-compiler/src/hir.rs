use crate::ast::{BinaryOp, Expr, Module, Param, Stmt, StructDecl, StructField, TypeRef, UnaryOp};
use crate::source::Span;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct HirModule {
    pub structs: Vec<HirStruct>,
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirFunction {
    pub id: usize,
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
pub struct HirParam {
    pub name: String,
    pub ty: Option<HirTypeRef>,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirTypeRef {
    pub name: String,
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
    Expr(HirExpr, Span),
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
        functions: module
            .functions
            .iter()
            .enumerate()
            .map(|(id, function)| HirFunction {
                id,
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
        Stmt::Expr(expr, span) => HirStmt::Expr(lower_expr(expr), *span),
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
            | HirExpr::Call { span, .. }
            | HirExpr::Field { span, .. }
            | HirExpr::Index { span, .. }
            | HirExpr::Go { span, .. }
            | HirExpr::Await { span, .. }
            | HirExpr::Unary { span, .. }
            | HirExpr::Binary { span, .. } => *span,
        }
    }
}
