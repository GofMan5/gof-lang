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
    Bool,
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
    If {
        condition: TypedExpr,
        then_body: Vec<TypedStmt>,
        else_body: Vec<TypedStmt>,
    },
    While {
        condition: TypedExpr,
        body: Vec<TypedStmt>,
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
    Bool(bool),
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

#[derive(Debug, Clone)]
struct ScopeStack {
    scopes: Vec<HashMap<String, LocalBinding>>,
}

impl ScopeStack {
    fn new(params: &[String]) -> Self {
        let root = params
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
        Self { scopes: vec![root] }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define_current(&mut self, name: String, binding: LocalBinding) {
        self.scopes
            .last_mut()
            .expect("scope stack should never be empty")
            .insert(name, binding);
    }

    fn contains_in_current(&self, name: &str) -> bool {
        self.scopes
            .last()
            .expect("scope stack should never be empty")
            .contains_key(name)
    }

    fn get(&self, name: &str) -> Option<LocalBinding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut LocalBinding> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }
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
    let mut scopes = ScopeStack::new(&function.params);
    let body = lower_block(&function.body, &mut scopes, signatures, diagnostics, false);
    let return_type = find_return_type(&body).unwrap_or(Type::Unit);

    TypedFunction {
        id: function.id,
        name: function.name.clone(),
        params: function.params.clone(),
        return_type,
        body,
    }
}

fn lower_block(
    stmts: &[HirStmt],
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, usize>,
    diagnostics: &mut Diagnostics,
    nested_scope: bool,
) -> Vec<TypedStmt> {
    if nested_scope {
        scopes.push();
    }

    let body = stmts
        .iter()
        .map(|stmt| lower_stmt(stmt, scopes, signatures, diagnostics))
        .collect::<Vec<_>>();

    if nested_scope {
        scopes.pop();
    }

    body
}

fn lower_stmt(
    stmt: &HirStmt,
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, usize>,
    diagnostics: &mut Diagnostics,
) -> TypedStmt {
    match stmt {
        HirStmt::Return(expr, _) => {
            TypedStmt::Return(lower_expr(expr, scopes, signatures, diagnostics))
        }
        HirStmt::Bind {
            name,
            mutable,
            value,
            span,
        } => {
            if scopes.contains_in_current(name) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3006",
                        format!("duplicate binding `{name}`"),
                        "gof currently does not allow duplicate bindings in the same block scope",
                        *span,
                    )
                    .with_fix_it("rename this binding or reuse the existing one"),
                );
            }

            let value = lower_expr(value, scopes, signatures, diagnostics);
            scopes.define_current(
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
            let value = lower_expr(value, scopes, signatures, diagnostics);
            if let Some(existing) = scopes.get_mut(name) {
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
                scopes.define_current(
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
        HirStmt::If {
            condition,
            then_body,
            else_body,
            span: _,
        } => {
            let condition = lower_expr(condition, scopes, signatures, diagnostics);
            ensure_bool_condition(&condition, diagnostics);
            let then_body = lower_block(then_body, scopes, signatures, diagnostics, true);
            let else_body = lower_block(else_body, scopes, signatures, diagnostics, true);
            TypedStmt::If {
                condition,
                then_body,
                else_body,
            }
        }
        HirStmt::While {
            condition,
            body,
            span: _,
        } => {
            let condition = lower_expr(condition, scopes, signatures, diagnostics);
            ensure_bool_condition(&condition, diagnostics);
            let body = lower_block(body, scopes, signatures, diagnostics, true);
            TypedStmt::While { condition, body }
        }
        HirStmt::Expr(expr, _) => {
            TypedStmt::Expr(lower_expr(expr, scopes, signatures, diagnostics))
        }
    }
}

fn ensure_bool_condition(condition: &TypedExpr, diagnostics: &mut Diagnostics) {
    if !matches!(condition.ty, Type::Bool | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3007",
                "condition must evaluate to `bool`",
                "control-flow conditions in gof currently require a boolean expression",
                condition.span,
            )
            .with_fix_it("use a comparison like `x > 0` or a boolean literal"),
        );
    }
}

fn lower_expr(
    expr: &HirExpr,
    scopes: &ScopeStack,
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
        HirExpr::Bool(value, span) => TypedExpr {
            kind: TypedExprKind::Bool(*value),
            ty: Type::Bool,
            span: *span,
        },
        HirExpr::Local(name, span) => {
            let ty = scopes
                .get(name)
                .map(|binding| binding.ty)
                .unwrap_or(Type::Unknown);
            if scopes.get(name).is_none() {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3002",
                        format!("unknown local `{name}`"),
                        "the identifier is not a parameter or a previously created binding in scope",
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
                .map(|arg| lower_expr(arg, scopes, signatures, diagnostics))
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
            let lhs = lower_expr(lhs, scopes, signatures, diagnostics);
            let rhs = lower_expr(rhs, scopes, signatures, diagnostics);
            let ty = infer_binary_type(lhs.ty, *op, rhs.ty);
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

fn infer_binary_type(lhs: Type, op: BinaryOp, rhs: Type) -> Type {
    match (lhs, op, rhs) {
        (Type::Int, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul, Type::Int) => Type::Int,
        (Type::String, BinaryOp::Add, Type::String) => Type::String,
        (
            Type::Int,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge,
            Type::Int,
        ) => Type::Bool,
        (Type::String, BinaryOp::Eq | BinaryOp::Ne, Type::String) => Type::Bool,
        (Type::Bool, BinaryOp::Eq | BinaryOp::Ne, Type::Bool) => Type::Bool,
        _ => Type::Unknown,
    }
}

fn find_return_type(stmts: &[TypedStmt]) -> Option<Type> {
    for stmt in stmts {
        match stmt {
            TypedStmt::Return(expr) => return Some(expr.ty),
            TypedStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if let Some(ty) =
                    find_return_type(then_body).or_else(|| find_return_type(else_body))
                {
                    return Some(ty);
                }
            }
            TypedStmt::While { body, .. } => {
                if let Some(ty) = find_return_type(body) {
                    return Some(ty);
                }
            }
            TypedStmt::Bind { .. } | TypedStmt::Assign { .. } | TypedStmt::Expr(_) => {}
        }
    }
    None
}
