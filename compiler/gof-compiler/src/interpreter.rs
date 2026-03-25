use crate::ast::{BinaryOp, Expr, Function, Module, Stmt};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    String(String),
    Bool(bool),
    Unit,
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    value: Value,
}

#[derive(Debug, Clone)]
struct ScopeStack {
    scopes: Vec<HashMap<String, Binding>>,
}

impl ScopeStack {
    fn new(params: &[String], args: &[Value]) -> Self {
        let root = params
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
        Self { scopes: vec![root] }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define_current(&mut self, name: String, binding: Binding) {
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

    fn get(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Binding> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }
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

    let mut scopes = ScopeStack::new(&function.params, args);
    if let Some(value) = eval_block(&function.body, &mut scopes, functions, false)? {
        Ok(value)
    } else {
        Ok(Value::Unit)
    }
}

fn eval_block(
    stmts: &[Stmt],
    scopes: &mut ScopeStack,
    functions: &HashMap<String, &Function>,
    nested_scope: bool,
) -> Result<Option<Value>, Diagnostics> {
    if nested_scope {
        scopes.push();
    }

    for stmt in stmts {
        if let Some(value) = eval_stmt(stmt, scopes, functions)? {
            if nested_scope {
                scopes.pop();
            }
            return Ok(Some(value));
        }
    }

    if nested_scope {
        scopes.pop();
    }

    Ok(None)
}

fn eval_stmt(
    stmt: &Stmt,
    scopes: &mut ScopeStack,
    functions: &HashMap<String, &Function>,
) -> Result<Option<Value>, Diagnostics> {
    match stmt {
        Stmt::Return(expr, _) => Ok(Some(eval_expr(expr, scopes, functions)?)),
        Stmt::Bind {
            name,
            mutable,
            value,
            span,
        } => {
            if scopes.contains_in_current(name) {
                return Err(Diagnostics(vec![Diagnostic::error(
                    "GOF3006",
                    format!("duplicate binding `{name}`"),
                    "gof currently does not allow duplicate bindings in the same block scope",
                    *span,
                )]));
            }

            let value = eval_expr(value, scopes, functions)?;
            scopes.define_current(
                name.clone(),
                Binding {
                    mutable: *mutable,
                    value,
                },
            );
            Ok(None)
        }
        Stmt::Assign { name, value, span } => {
            let value = eval_expr(value, scopes, functions)?;
            if let Some(existing) = scopes.get_mut(name) {
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
                scopes.define_current(
                    name.clone(),
                    Binding {
                        mutable: false,
                        value,
                    },
                );
            }
            Ok(None)
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let condition = eval_expr(condition, scopes, functions)?;
            match condition {
                Value::Bool(true) => eval_block(then_body, scopes, functions, true),
                Value::Bool(false) => eval_block(else_body, scopes, functions, true),
                _ => Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3007",
                        "condition must evaluate to `bool`",
                        "control-flow conditions in gof currently require a boolean expression",
                        condition_span(stmt),
                    )
                    .with_fix_it("use a comparison like `x > 0` or a boolean literal"),
                ])),
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            loop {
                let value = eval_expr(condition, scopes, functions)?;
                match value {
                    Value::Bool(true) => {
                        if let Some(result) = eval_block(body, scopes, functions, true)? {
                            return Ok(Some(result));
                        }
                    }
                    Value::Bool(false) => break,
                    _ => {
                        return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3007",
                            "condition must evaluate to `bool`",
                            "control-flow conditions in gof currently require a boolean expression",
                            condition_span(stmt),
                        )
                        .with_fix_it("use a comparison like `x > 0` or a boolean literal"),
                    ]));
                    }
                }
            }
            Ok(None)
        }
        Stmt::Expr(expr, _) => {
            let _ = eval_expr(expr, scopes, functions)?;
            Ok(None)
        }
    }
}

fn eval_expr(
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &HashMap<String, &Function>,
) -> Result<Value, Diagnostics> {
    match expr {
        Expr::Int(value, _) => Ok(Value::Int(*value)),
        Expr::String(value, _) => Ok(Value::String(value.clone())),
        Expr::Bool(value, _) => Ok(Value::Bool(*value)),
        Expr::Ident(name, span) => scopes
            .get(name)
            .map(|binding| binding.value.clone())
            .ok_or_else(|| {
                Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3002",
                        format!("unknown local `{name}`"),
                        "the identifier is not a parameter or a previously created binding in scope",
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
                .map(|arg| eval_expr(arg, scopes, functions))
                .collect::<Result<Vec<_>, _>>()?;
            eval_function(function, &values, functions)
        }
        Expr::Binary { lhs, op, rhs, span } => {
            let lhs = eval_expr(lhs, scopes, functions)?;
            let rhs = eval_expr(rhs, scopes, functions)?;
            eval_binary(lhs, *op, rhs, *span)
        }
    }
}

fn eval_binary(lhs: Value, op: BinaryOp, rhs: Value, span: Span) -> Result<Value, Diagnostics> {
    match (lhs, op, rhs) {
        (Value::Int(lhs), BinaryOp::Add, Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
        (Value::Int(lhs), BinaryOp::Sub, Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
        (Value::Int(lhs), BinaryOp::Mul, Value::Int(rhs)) => Ok(Value::Int(lhs * rhs)),
        (Value::String(lhs), BinaryOp::Add, Value::String(rhs)) => Ok(Value::String(lhs + &rhs)),
        (Value::Int(lhs), BinaryOp::Eq, Value::Int(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Int(lhs), BinaryOp::Ne, Value::Int(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Int(lhs), BinaryOp::Lt, Value::Int(rhs)) => Ok(Value::Bool(lhs < rhs)),
        (Value::Int(lhs), BinaryOp::Le, Value::Int(rhs)) => Ok(Value::Bool(lhs <= rhs)),
        (Value::Int(lhs), BinaryOp::Gt, Value::Int(rhs)) => Ok(Value::Bool(lhs > rhs)),
        (Value::Int(lhs), BinaryOp::Ge, Value::Int(rhs)) => Ok(Value::Bool(lhs >= rhs)),
        (Value::String(lhs), BinaryOp::Eq, Value::String(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::String(lhs), BinaryOp::Ne, Value::String(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Bool(lhs), BinaryOp::Eq, Value::Bool(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Bool(lhs), BinaryOp::Ne, Value::Bool(rhs)) => Ok(Value::Bool(lhs != rhs)),
        _ => Err(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "the current evaluator only supports int arithmetic, comparisons, and string/bool equality",
            span,
        )])),
    }
}

fn condition_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::If { condition, .. } | Stmt::While { condition, .. } => condition.span(),
        _ => Span::new(1, 1, 1),
    }
}
