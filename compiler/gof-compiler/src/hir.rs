use crate::ast::{BinaryOp, Expr, Module, Stmt};
use crate::source::Span;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HirFunction {
    pub id: usize,
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub enum HirStmt {
    Return(HirExpr, Span),
    Bind {
        name: String,
        mutable: bool,
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
    Call {
        callee: String,
        args: Vec<HirExpr>,
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
        functions: module
            .functions
            .iter()
            .enumerate()
            .map(|(id, function)| HirFunction {
                id,
                name: function.name.clone(),
                params: function.params.clone(),
                body: function.body.iter().map(lower_stmt).collect(),
            })
            .collect(),
    }
}

fn lower_stmt(stmt: &Stmt) -> HirStmt {
    match stmt {
        Stmt::Return(expr, span) => HirStmt::Return(lower_expr(expr), *span),
        Stmt::Bind {
            name,
            mutable,
            value,
            span,
        } => HirStmt::Bind {
            name: name.clone(),
            mutable: *mutable,
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
        Expr::Call { callee, args, span } => HirExpr::Call {
            callee: callee.clone(),
            args: args.iter().map(lower_expr).collect(),
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
            | HirExpr::Call { span, .. }
            | HirExpr::Binary { span, .. } => *span,
        }
    }
}
