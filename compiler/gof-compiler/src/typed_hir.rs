use crate::ast::BinaryOp;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::hir::{HirExpr, HirFunction, HirModule, HirStmt};
use crate::source::Span;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Type {
    Int,
    String,
    Unknown,
    Unit,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedModule {
    pub functions: Vec<TypedFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedFunction {
    pub id: usize,
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Type,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TypedStmt {
    Return(TypedExpr),
    Bind {
        name: String,
        mutable: bool,
        value: TypedExpr,
    },
    Assign {
        name: String,
        value: TypedExpr,
    },
    Expr(TypedExpr),
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, Serialize)]
pub enum TypedExprKind {
    Int(i64),
    String(String),
    Local(String),
    Call {
        callee: String,
        args: Vec<TypedExpr>,
    },
    Binary {
        lhs: Box<TypedExpr>,
        op: BinaryOp,
        rhs: Box<TypedExpr>,
    },
}

#[derive(Debug, Clone, Copy)]
struct LocalBinding {
    mutable: bool,
    ty: Type,
}

pub fn lower(module: &HirModule) -> Result<TypedModule, Diagnostics> {
    let signatures = module
        .functions
        .iter()
        .map(|function| (function.name.clone(), function.params.len()))
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Diagnostics::default();

    let functions = module
        .functions
        .iter()
        .map(|function| lower_function(function, &signatures, &mut diagnostics))
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        Ok(TypedModule { functions })
    } else {
        Err(diagnostics)
    }
}

fn lower_function(
    function: &HirFunction,
    signatures: &HashMap<String, usize>,
    diagnostics: &mut Diagnostics,
) -> TypedFunction {
    let mut locals = function
        .params
        .iter()
        .cloned()
        .map(|name| {
            (
                name,
                LocalBinding {
                    mutable: false,
                    ty: Type::Unknown,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let body = function
        .body
        .iter()
        .map(|stmt| lower_stmt(stmt, &mut locals, signatures, diagnostics))
        .collect::<Vec<_>>();

    let return_type = body
        .iter()
        .find_map(|stmt| match stmt {
            TypedStmt::Return(expr) => Some(expr.ty),
            TypedStmt::Bind { .. } | TypedStmt::Assign { .. } | TypedStmt::Expr(_) => None,
        })
        .unwrap_or(Type::Unit);

    TypedFunction {
        id: function.id,
        name: function.name.clone(),
        params: function.params.clone(),
        return_type,
        body,
    }
}

fn lower_stmt(
    stmt: &HirStmt,
    locals: &mut HashMap<String, LocalBinding>,
    signatures: &HashMap<String, usize>,
    diagnostics: &mut Diagnostics,
) -> TypedStmt {
    match stmt {
        HirStmt::Return(expr, _) => {
            TypedStmt::Return(lower_expr(expr, locals, signatures, diagnostics))
        }
        HirStmt::Bind {
            name,
            mutable,
            value,
            span,
        } => {
            if locals.contains_key(name) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3006",
                        format!("duplicate binding `{name}`"),
                        "gof currently does not allow shadowing in the same function scope",
                        *span,
                    )
                    .with_fix_it("rename this binding or reuse the existing one"),
                );
            }

            let value = lower_expr(value, locals, signatures, diagnostics);
            locals.insert(
                name.clone(),
                LocalBinding {
                    mutable: *mutable,
                    ty: value.ty,
                },
            );

            TypedStmt::Bind {
                name: name.clone(),
                mutable: *mutable,
                value,
            }
        }
        HirStmt::Assign { name, value, span } => {
            let value = lower_expr(value, locals, signatures, diagnostics);

            if let Some(existing) = locals.get_mut(name) {
                if !existing.mutable {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3003",
                            format!("cannot reassign immutable binding `{name}`"),
                            "bindings declared without `mut` are immutable after their first assignment",
                            *span,
                        )
                        .with_fix_it("declare the binding as `mut name = ...` before reassigning it"),
                    );
                } else if existing.ty == Type::Unknown {
                    existing.ty = value.ty;
                }

                TypedStmt::Assign {
                    name: name.clone(),
                    value,
                }
            } else {
                locals.insert(
                    name.clone(),
                    LocalBinding {
                        mutable: false,
                        ty: value.ty,
                    },
                );
                TypedStmt::Bind {
                    name: name.clone(),
                    mutable: false,
                    value,
                }
            }
        }
        HirStmt::Expr(expr, _) => {
            TypedStmt::Expr(lower_expr(expr, locals, signatures, diagnostics))
        }
    }
}

fn lower_expr(
    expr: &HirExpr,
    locals: &HashMap<String, LocalBinding>,
    signatures: &HashMap<String, usize>,
    diagnostics: &mut Diagnostics,
) -> TypedExpr {
    match expr {
        HirExpr::Int(value, span) => TypedExpr {
            kind: TypedExprKind::Int(*value),
            ty: Type::Int,
            span: *span,
        },
        HirExpr::String(value, span) => TypedExpr {
            kind: TypedExprKind::String(value.clone()),
            ty: Type::String,
            span: *span,
        },
        HirExpr::Local(name, span) => {
            let ty = locals
                .get(name)
                .map(|binding| binding.ty)
                .unwrap_or(Type::Unknown);
            if !locals.contains_key(name) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3002",
                        format!("unknown local `{name}`"),
                        "the identifier is not a parameter or a previously created binding",
                        *span,
                    )
                    .with_fix_it("define the binding before using it"),
                );
            }
            TypedExpr {
                kind: TypedExprKind::Local(name.clone()),
                ty,
                span: *span,
            }
        }
        HirExpr::Call { callee, args, span } => {
            let typed_args = args
                .iter()
                .map(|arg| lower_expr(arg, locals, signatures, diagnostics))
                .collect::<Vec<_>>();
            match signatures.get(callee) {
                Some(expected) if *expected == typed_args.len() => {}
                Some(expected) => diagnostics.push(
                    Diagnostic::error(
                        "GOF3005",
                        format!("wrong number of arguments for `{callee}`"),
                        format!("expected {expected} argument(s), got {}", typed_args.len()),
                        *span,
                    )
                    .with_fix_it("pass the exact number of parameters declared by the function"),
                ),
                None => diagnostics.push(
                    Diagnostic::error(
                        "GOF3004",
                        format!("unknown function `{callee}`"),
                        "only top-level named functions can be called in the bootstrap compiler",
                        *span,
                    )
                    .with_fix_it("define the function before calling it"),
                ),
            }

            TypedExpr {
                kind: TypedExprKind::Call {
                    callee: callee.clone(),
                    args: typed_args,
                },
                ty: Type::Unknown,
                span: *span,
            }
        }
        HirExpr::Binary { lhs, op, rhs, span } => {
            let lhs = lower_expr(lhs, locals, signatures, diagnostics);
            let rhs = lower_expr(rhs, locals, signatures, diagnostics);
            let ty = match (lhs.ty, rhs.ty, op) {
                (Type::Int, Type::Int, BinaryOp::Add | BinaryOp::Sub) => Type::Int,
                (Type::String, Type::String, BinaryOp::Add) => Type::String,
                _ => Type::Unknown,
            };
            TypedExpr {
                kind: TypedExprKind::Binary {
                    lhs: Box::new(lhs),
                    op: *op,
                    rhs: Box::new(rhs),
                },
                ty,
                span: *span,
            }
        }
    }
}
