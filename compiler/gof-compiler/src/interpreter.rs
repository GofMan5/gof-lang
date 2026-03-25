use crate::ast::{BinaryOp, Expr, Function, Module, Stmt};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    String(String),
    Unit,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    value: Value,
}

pub fn run(module: &Module) -> Result<Value, Diagnostics> {
    let functions = module
        .functions
        .iter()
        .map(|function| (function.name.clone(), function))
        .collect::<HashMap<_, _>>();

    let main = functions.get("main").ok_or_else(|| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3001",
                "missing `main` entrypoint",
                "executable mode requires a top-level `fn main():`",
                Span::new(1, 1, 1),
            )
            .with_fix_it("add `fn main():` as the program entrypoint"),
        ])
    })?;

    if !main.params.is_empty() {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3001",
                "`main` cannot take parameters in bootstrap run mode",
                "the evaluator only supports zero-argument entrypoints for now",
                Span::new(1, 1, 1),
            )
            .with_fix_it("remove the parameters from `main`"),
        ]));
    }

    eval_function(main, &[], &functions)
}

fn eval_function(
    function: &Function,
    args: &[Value],
    functions: &HashMap<String, &Function>,
) -> Result<Value, Diagnostics> {
    if function.params.len() != args.len() {
        return Err(Diagnostics(vec![Diagnostic::error(
            "GOF3005",
            format!("wrong number of arguments for `{}`", function.name),
            format!(
                "expected {} argument(s), got {}",
                function.params.len(),
                args.len()
            ),
            function.span,
        )]));
    }

    let mut locals = function
        .params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .map(|(name, value)| {
            (
                name,
                Binding {
                    mutable: false,
                    value,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for stmt in &function.body {
        if let Some(value) = eval_stmt(stmt, &mut locals, functions)? {
            return Ok(value);
        }
    }

    Ok(Value::Unit)
}

fn eval_stmt(
    stmt: &Stmt,
    locals: &mut HashMap<String, Binding>,
    functions: &HashMap<String, &Function>,
) -> Result<Option<Value>, Diagnostics> {
    match stmt {
        Stmt::Return(expr, _) => Ok(Some(eval_expr(expr, locals, functions)?)),
        Stmt::Bind {
            name,
            mutable,
            value,
            span,
        } => {
            if locals.contains_key(name) {
                return Err(Diagnostics(vec![Diagnostic::error(
                    "GOF3006",
                    format!("duplicate binding `{name}`"),
                    "gof currently does not allow shadowing in the same function scope",
                    *span,
                )]));
            }

            let value = eval_expr(value, locals, functions)?;
            locals.insert(
                name.clone(),
                Binding {
                    mutable: *mutable,
                    value,
                },
            );
            Ok(None)
        }
        Stmt::Assign { name, value, span } => {
            let value = eval_expr(value, locals, functions)?;
            if let Some(existing) = locals.get_mut(name) {
                if !existing.mutable {
                    return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3003",
                            format!("cannot reassign immutable binding `{name}`"),
                            "bindings declared without `mut` are immutable after their first assignment",
                            *span,
                        )
                        .with_fix_it("declare the binding as `mut name = ...` before reassigning it"),
                    ]));
                }
                existing.value = value;
            } else {
                locals.insert(
                    name.clone(),
                    Binding {
                        mutable: false,
                        value,
                    },
                );
            }
            Ok(None)
        }
        Stmt::Expr(expr, _) => {
            let _ = eval_expr(expr, locals, functions)?;
            Ok(None)
        }
    }
}

fn eval_expr(
    expr: &Expr,
    locals: &HashMap<String, Binding>,
    functions: &HashMap<String, &Function>,
) -> Result<Value, Diagnostics> {
    match expr {
        Expr::Int(value, _) => Ok(Value::Int(*value)),
        Expr::String(value, _) => Ok(Value::String(value.clone())),
        Expr::Ident(name, span) => locals
            .get(name)
            .map(|binding| binding.value.clone())
            .ok_or_else(|| {
                Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3002",
                        format!("unknown local `{name}`"),
                        "the identifier is not a parameter or a previously created binding",
                        *span,
                    )
                    .with_fix_it("define the binding before using it"),
                ])
            }),
        Expr::Call { callee, args, span } => {
            let function = functions.get(callee).ok_or_else(|| {
                Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3004",
                        format!("unknown function `{callee}`"),
                        "only top-level named functions can be called in the bootstrap evaluator",
                        *span,
                    )
                    .with_fix_it("define the function before calling it"),
                ])
            })?;
            let values = args
                .iter()
                .map(|arg| eval_expr(arg, locals, functions))
                .collect::<Result<Vec<_>, _>>()?;
            eval_function(function, &values, functions)
        }
        Expr::Binary { lhs, op, rhs, span } => {
            let lhs = eval_expr(lhs, locals, functions)?;
            let rhs = eval_expr(rhs, locals, functions)?;
            eval_binary(lhs, *op, rhs, *span)
        }
    }
}

fn eval_binary(lhs: Value, op: BinaryOp, rhs: Value, span: Span) -> Result<Value, Diagnostics> {
    match (lhs, op, rhs) {
        (Value::Int(lhs), BinaryOp::Add, Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
        (Value::Int(lhs), BinaryOp::Sub, Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
        (Value::String(lhs), BinaryOp::Add, Value::String(rhs)) => Ok(Value::String(lhs + &rhs)),
        _ => Err(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "only int +/- int and string + string are supported right now",
            span,
        )])),
    }
}
