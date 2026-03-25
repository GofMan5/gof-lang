use crate::ast::{BinaryOp, Expr, Function, Module, Param, Stmt};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

type FunctionTable = Arc<HashMap<String, Function>>;

#[derive(Clone)]
pub enum Value {
    Int(i64),
    String(String),
    Bool(bool),
    List(Vec<Value>),
    Task(TaskValue),
    Unit,
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => f.debug_tuple("Int").field(value).finish(),
            Self::String(value) => f.debug_tuple("String").field(value).finish(),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::List(values) => f.debug_tuple("List").field(values).finish(),
            Self::Task(_) => f.write_str("Task(<pending-or-completed>)"),
            Self::Unit => f.write_str("Unit"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(lhs), Self::Int(rhs)) => lhs == rhs,
            (Self::String(lhs), Self::String(rhs)) => lhs == rhs,
            (Self::Bool(lhs), Self::Bool(rhs)) => lhs == rhs,
            (Self::List(lhs), Self::List(rhs)) => lhs == rhs,
            (Self::Task(lhs), Self::Task(rhs)) => lhs.ptr_eq(rhs),
            (Self::Unit, Self::Unit) => true,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Value {
    pub fn cli_text(&self) -> Option<String> {
        match self {
            Self::Int(value) => Some(value.to_string()),
            Self::String(value) => Some(value.clone()),
            Self::Bool(value) => Some(value.to_string()),
            Self::List(values) => Some(format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| value.cli_text().unwrap_or_else(|| "unit".to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Self::Task(_) => Some("<task>".to_string()),
            Self::Unit => None,
        }
    }
}

#[derive(Clone)]
pub struct TaskValue(Arc<TaskHandle>);

impl TaskValue {
    fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn await_value(&self) -> Result<Value, Diagnostics> {
        self.0.await_value()
    }
}

#[derive(Debug)]
struct TaskHandle {
    result: Arc<(Mutex<Option<Result<Value, Diagnostics>>>, Condvar)>,
}

impl TaskHandle {
    fn new() -> Self {
        Self {
            result: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    fn store(&self, result: Result<Value, Diagnostics>) {
        let (lock, ready) = &*self.result;
        let mut slot = lock
            .lock()
            .expect("task result mutex should not be poisoned");
        *slot = Some(result);
        ready.notify_all();
    }

    fn await_value(&self) -> Result<Value, Diagnostics> {
        let (lock, ready) = &*self.result;
        let mut slot = lock
            .lock()
            .expect("task result mutex should not be poisoned");
        while slot.is_none() {
            slot = ready
                .wait(slot)
                .expect("task result wait should not be poisoned");
        }
        slot.as_ref()
            .expect("task result should exist after wait")
            .clone()
    }
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
    fn new(params: &[Param], args: &[Value]) -> Self {
        let root = params
            .iter()
            .map(|param| param.name.clone())
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
    let functions = Arc::new(
        module
            .functions
            .iter()
            .map(|function| (function.name.clone(), function.clone()))
            .collect::<HashMap<_, _>>(),
    );

    let main = functions.get("main").cloned().ok_or_else(|| {
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

    eval_function(&main, &[], &functions)
}

fn eval_function(
    function: &Function,
    args: &[Value],
    functions: &FunctionTable,
) -> Result<Value, Diagnostics> {
    if function.params.len() != args.len() {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{}`", function.name),
                format!(
                    "expected {} argument(s), got {}",
                    function.params.len(),
                    args.len()
                ),
                function.span,
            )
            .with_source_path(function.source_path.clone()),
        ]));
    }

    let mut scopes = ScopeStack::new(&function.params, args);
    if let Some(value) = eval_block(
        &function.body,
        &mut scopes,
        functions,
        false,
        &function.source_path,
    )? {
        Ok(value)
    } else {
        Ok(Value::Unit)
    }
}

fn eval_block(
    stmts: &[Stmt],
    scopes: &mut ScopeStack,
    functions: &FunctionTable,
    nested_scope: bool,
    source_path: &Path,
) -> Result<Option<Value>, Diagnostics> {
    if nested_scope {
        scopes.push();
    }

    for stmt in stmts {
        if let Some(value) = eval_stmt(stmt, scopes, functions, source_path)? {
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
    functions: &FunctionTable,
    source_path: &Path,
) -> Result<Option<Value>, Diagnostics> {
    match stmt {
        Stmt::Return(expr, _) => Ok(Some(eval_expr(expr, scopes, functions, source_path)?)),
        Stmt::Bind {
            name,
            mutable,
            value,
            span,
            ..
        } => {
            if scopes.contains_in_current(name) {
                return Err(Diagnostics(vec![Diagnostic::error(
                    "GOF3006",
                    format!("duplicate binding `{name}`"),
                    "gof currently does not allow duplicate bindings in the same block scope",
                    *span,
                )]));
            }

            let value = eval_expr(value, scopes, functions, source_path)?;
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
            let value = eval_expr(value, scopes, functions, source_path)?;
            if let Some(existing) = scopes.get_mut(name) {
                if !existing.mutable {
                    return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3003",
                            format!("cannot reassign immutable binding `{name}`"),
                            "bindings declared without `mut` are immutable after their first assignment",
                            *span,
                        )
                        .with_fix_it("declare the binding as `mut name = ...` before reassigning it")
                        .with_source_path(source_path.to_path_buf()),
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
            let condition = eval_expr(condition, scopes, functions, source_path)?;
            match condition {
                Value::Bool(true) => eval_block(then_body, scopes, functions, true, source_path),
                Value::Bool(false) => eval_block(else_body, scopes, functions, true, source_path),
                _ => Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3007",
                        "condition must evaluate to `bool`",
                        "control-flow conditions in gof currently require a boolean expression",
                        condition_span(stmt),
                    )
                    .with_fix_it("use a comparison like `x > 0` or a boolean literal")
                    .with_source_path(source_path.to_path_buf()),
                ])),
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            loop {
                let value = eval_expr(condition, scopes, functions, source_path)?;
                match value {
                    Value::Bool(true) => {
                        if let Some(result) =
                            eval_block(body, scopes, functions, true, source_path)?
                        {
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
                            .with_fix_it("use a comparison like `x > 0` or a boolean literal")
                            .with_source_path(source_path.to_path_buf()),
                        ]));
                    }
                }
            }
            Ok(None)
        }
        Stmt::Expr(expr, _) => {
            let _ = eval_expr(expr, scopes, functions, source_path)?;
            Ok(None)
        }
    }
}

fn eval_expr(
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    match expr {
        Expr::Int(value, _) => Ok(Value::Int(*value)),
        Expr::String(value, _) => Ok(Value::String(value.clone())),
        Expr::Bool(value, _) => Ok(Value::Bool(*value)),
        Expr::List { items, .. } => {
            let values = items
                .iter()
                .map(|item| eval_expr(item, scopes, functions, source_path))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(values))
        }
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
                    .with_fix_it("define the binding before using it")
                    .with_source_path(source_path.to_path_buf()),
                ])
            }),
        Expr::Call { callee, args, span } => {
            if callee == "len" {
                return eval_len_builtin(args, scopes, functions, source_path, *span);
            }

            let function = functions.get(callee).cloned().ok_or_else(|| {
                Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3004",
                        format!("unknown function `{callee}`"),
                        "only top-level named functions can be called in the bootstrap evaluator",
                        *span,
                    )
                    .with_fix_it("define the function before calling it")
                    .with_source_path(source_path.to_path_buf()),
                ])
            })?;
            let values = args
                .iter()
                .map(|arg| eval_expr(arg, scopes, functions, source_path))
                .collect::<Result<Vec<_>, _>>()?;
            eval_function(&function, &values, functions)
        }
        Expr::Index {
            target,
            index,
            span,
        } => {
            let target = eval_expr(target, scopes, functions, source_path)?;
            let index = eval_expr(index, scopes, functions, source_path)?;
            eval_index(target, index, *span, source_path)
        }
        Expr::Go { value, span } => match value.as_ref() {
            Expr::Call { callee, args, span } => {
                let values = args
                    .iter()
                    .map(|arg| eval_expr(arg, scopes, functions, source_path))
                    .collect::<Result<Vec<_>, _>>()?;
                spawn_task(callee, values, *span, functions, source_path)
            }
            _ => Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3008",
                    "`go` currently requires a named function call",
                    "the bootstrap concurrency model only supports `go some_fn(...)` for top-level functions",
                    *span,
                )
                .with_fix_it("replace this expression with `go some_function(...)`")
                .with_source_path(source_path.to_path_buf()),
            ])),
        },
        Expr::Await { value, span } => match eval_expr(value, scopes, functions, source_path)? {
            Value::Task(task) => task.await_value(),
            _ => Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3009",
                    "`await` requires a task value",
                    "only values produced by `go` can currently be awaited in the bootstrap evaluator",
                    *span,
                )
                .with_fix_it("store `go some_function(...)` in a binding and await that task")
                .with_source_path(source_path.to_path_buf()),
            ])),
        },
        Expr::Binary { lhs, op, rhs, span } => {
            let lhs = eval_expr(lhs, scopes, functions, source_path)?;
            let rhs = eval_expr(rhs, scopes, functions, source_path)?;
            eval_binary(lhs, *op, rhs, *span, source_path)
        }
    }
}

fn spawn_task(
    callee: &str,
    args: Vec<Value>,
    span: Span,
    functions: &FunctionTable,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    let function = functions.get(callee).cloned().ok_or_else(|| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3004",
                format!("unknown function `{callee}`"),
                "only top-level named functions can be called in the bootstrap evaluator",
                span,
            )
            .with_fix_it("define the function before calling it")
            .with_source_path(source_path.to_path_buf()),
        ])
    })?;

    let task = Arc::new(TaskHandle::new());
    let task_handle = Arc::clone(&task);
    let function_name = callee.to_string();
    let functions = Arc::clone(functions);

    std::thread::spawn(move || {
        let result = match std::panic::catch_unwind(AssertUnwindSafe(|| {
            eval_function(&function, &args, &functions)
        })) {
            Ok(result) => result,
            Err(_) => Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3010",
                    format!("task `{function_name}` panicked"),
                    "a spawned task hit an internal failure before producing a value",
                    span,
                )
                .with_fix_it("inspect the spawned function and remove invariant-breaking panics")
                .with_source_path(function.source_path.clone()),
            ])),
        };
        task_handle.store(result);
    });

    Ok(Value::Task(TaskValue(task)))
}

fn eval_binary(
    lhs: Value,
    op: BinaryOp,
    rhs: Value,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
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
        (Value::List(lhs), BinaryOp::Eq, Value::List(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::List(lhs), BinaryOp::Ne, Value::List(rhs)) => Ok(Value::Bool(lhs != rhs)),
        _ => Err(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "the current evaluator only supports int arithmetic, comparisons, and string/bool equality",
            span,
        )
        .with_source_path(source_path.to_path_buf())])),
    }
}

fn eval_len_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 1 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `len`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `len` with exactly one list or string argument")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let value = eval_expr(&args[0], scopes, functions, source_path)?;
    match value {
        Value::List(values) => Ok(Value::Int(values.len() as i64)),
        Value::String(value) => Ok(Value::Int(value.chars().count() as i64)),
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3019",
                "`len` requires a list or string value",
                format!("this argument resolves to `{}`", value_name(&other)),
                args[0].span(),
            )
            .with_fix_it("pass a list literal, list binding, or string value to `len`")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn eval_index(
    target: Value,
    index: Value,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    let Value::Int(index) = index else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "list indexing requires an `int` index",
                format!("this index resolves to `{}`", value_name(&index)),
                span,
            )
            .with_fix_it("use an integer index like `values[0]`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if index < 0 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "list indexing requires a non-negative index",
                "negative indices are not supported in the bootstrap evaluator",
                span,
            )
            .with_fix_it("use an index between `0` and `len(list) - 1`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    match target {
        Value::List(values) => values.get(index as usize).cloned().ok_or_else(|| {
            Diagnostics(vec![
                Diagnostic::error(
                    "GOF3018",
                    "list index is out of bounds",
                    format!(
                        "the list length is {}, but the index is {}",
                        values.len(),
                        index
                    ),
                    span,
                )
                .with_fix_it("keep the index below `len(list)`")
                .with_source_path(source_path.to_path_buf()),
            ])
        }),
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "indexing requires a list value",
                format!("this target resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("index only list literals or list bindings")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn value_name(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "int",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::List(_) => "list",
        Value::Task(_) => "task",
        Value::Unit => "unit",
    }
}

fn condition_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::If { condition, .. } | Stmt::While { condition, .. } => condition.span(),
        _ => Span::new(1, 1, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, run};
    use crate::ast::parse;
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;

    fn run_source(text: &str) -> Result<Value, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("test.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        run(&module)
    }

    #[test]
    fn evaluates_go_and_await() {
        let value = run_source(
            "fn square(x):\n    return x * x\nfn main():\n    left = go square(5)\n    right = go square(4)\n    return await left + await right\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(41));
    }

    #[test]
    fn rejects_await_on_non_task() {
        let diagnostics =
            run_source("fn main():\n    return await 42\n").expect_err("await should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3009"]);
    }

    #[test]
    fn evaluates_lists_indexing_and_len() {
        let value = run_source(
            "fn main() -> int:\n    values = [2, 4, 6]\n    return values[1] + len(values)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(7));
    }
}
