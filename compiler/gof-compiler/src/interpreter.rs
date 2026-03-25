use crate::ast::{BinaryOp, EnumDecl, Expr, Function, Module, Param, Stmt, StructDecl, UnaryOp};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};

type FunctionTable = Arc<HashMap<String, Function>>;
type MethodTable = Arc<HashMap<(String, String), Function>>;
type StructTable = Arc<HashMap<String, StructDecl>>;
type EnumTable = Arc<HashMap<String, EnumDecl>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub value: Value,
    pub stdout: String,
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    String(String),
    Bool(bool),
    Struct(StructValue),
    Enum(EnumValue),
    List(Vec<Value>),
    Dict(DictValue),
    Channel(ChannelValue),
    Task(TaskValue),
    Unit,
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => f.debug_tuple("Int").field(value).finish(),
            Self::String(value) => f.debug_tuple("String").field(value).finish(),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Struct(value) => f.debug_tuple("Struct").field(value).finish(),
            Self::Enum(value) => f.debug_tuple("Enum").field(value).finish(),
            Self::List(values) => f.debug_tuple("List").field(values).finish(),
            Self::Dict(value) => f.debug_tuple("Dict").field(value).finish(),
            Self::Channel(_) => f.write_str("Channel(<open>)"),
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
            (Self::Struct(lhs), Self::Struct(rhs)) => lhs == rhs,
            (Self::Enum(lhs), Self::Enum(rhs)) => lhs == rhs,
            (Self::List(lhs), Self::List(rhs)) => lhs == rhs,
            (Self::Dict(lhs), Self::Dict(rhs)) => lhs == rhs,
            (Self::Channel(lhs), Self::Channel(rhs)) => lhs.ptr_eq(rhs),
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
            Self::Struct(value) => Some(value.cli_text()),
            Self::Enum(value) => Some(value.cli_text()),
            Self::List(values) => Some(format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| value.cli_text().unwrap_or_else(|| "unit".to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Self::Dict(value) => Some(value.cli_text()),
            Self::Channel(_) => Some("<channel>".to_string()),
            Self::Task(_) => Some("<task>".to_string()),
            Self::Unit => None,
        }
    }

    fn printable_text(&self) -> Option<String> {
        match self {
            Self::Channel(_) | Self::Task(_) | Self::Unit => None,
            _ => self.cli_text(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictValue {
    entries: BTreeMap<String, Value>,
}

impl DictValue {
    fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    fn insert(&self, key: String, value: Value) -> Self {
        let mut entries = self.entries.clone();
        entries.insert(key, value);
        Self { entries }
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn cli_text(&self) -> String {
        format!(
            "{{{}}}",
            self.entries
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{key:?}: {}",
                        value.cli_text().unwrap_or_else(|| "unit".to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructValue {
    name: String,
    fields: Vec<(String, Value)>,
}

impl StructValue {
    fn field(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find_map(|(field_name, value)| (field_name == name).then_some(value))
    }

    fn cli_text(&self) -> String {
        format!(
            "{}({})",
            self.name,
            self.fields
                .iter()
                .map(|(field, value)| {
                    format!(
                        "{field}: {}",
                        value.cli_text().unwrap_or_else(|| "unit".to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumValue {
    name: String,
    variant: String,
}

impl EnumValue {
    fn cli_text(&self) -> String {
        format!("{}.{}", self.name, self.variant)
    }
}

#[derive(Clone)]
pub struct ChannelValue(Arc<ChannelHandle>);

impl ChannelValue {
    fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn send(&self, value: Value, span: Span, source_path: &Path) -> Result<(), Diagnostics> {
        self.0.send(value, span, source_path)
    }

    fn recv(&self, span: Span, source_path: &Path) -> Result<Value, Diagnostics> {
        self.0.recv(span, source_path)
    }

    fn try_recv(&self, span: Span, source_path: &Path) -> Result<Option<Value>, Diagnostics> {
        self.0.try_recv(span, source_path)
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

#[derive(Debug)]
struct ChannelHandle {
    sender: Sender<Value>,
    receiver: Mutex<Receiver<Value>>,
}

#[derive(Clone, Default)]
struct OutputBuffer(Arc<Mutex<String>>);

impl OutputBuffer {
    fn push_line(&self, line: &str) {
        let mut buffer = self
            .0
            .lock()
            .expect("output buffer mutex should not be poisoned");
        buffer.push_str(line);
        buffer.push('\n');
    }

    fn snapshot(&self) -> String {
        self.0
            .lock()
            .expect("output buffer mutex should not be poisoned")
            .clone()
    }
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

impl ChannelHandle {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }

    fn send(&self, value: Value, span: Span, source_path: &Path) -> Result<(), Diagnostics> {
        self.sender.send(value).map_err(|_| {
            Diagnostics(vec![
                Diagnostic::error(
                    "GOF3046",
                    "failed to send on channel",
                    "the bootstrap channel is no longer available for sending",
                    span,
                )
                .with_fix_it("keep the channel alive until all sends complete")
                .with_source_path(source_path.to_path_buf()),
            ])
        })
    }

    fn recv(&self, span: Span, source_path: &Path) -> Result<Value, Diagnostics> {
        self.receiver
            .lock()
            .expect("channel receiver mutex should not be poisoned")
            .recv()
            .map_err(|_| {
                Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3046",
                        "failed to receive from channel",
                        "the bootstrap channel was closed before a value arrived",
                        span,
                    )
                    .with_fix_it(
                        "ensure another task sends a value before all channel handles drop",
                    )
                    .with_source_path(source_path.to_path_buf()),
                ])
            })
    }

    fn try_recv(&self, span: Span, source_path: &Path) -> Result<Option<Value>, Diagnostics> {
        match self
            .receiver
            .lock()
            .expect("channel receiver mutex should not be poisoned")
            .try_recv()
        {
            Ok(value) => Ok(Some(value)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3046",
                    "failed to receive from channel",
                    "the bootstrap channel was closed before a value arrived",
                    span,
                )
                .with_fix_it("ensure another task sends a value before all channel handles drop")
                .with_source_path(source_path.to_path_buf()),
            ])),
        }
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
    Ok(run_with_output(module)?.value)
}

pub fn run_with_output(module: &Module) -> Result<ExecutionResult, Diagnostics> {
    let functions = Arc::new(
        module
            .functions
            .iter()
            .filter(|function| function.receiver_type.is_none())
            .map(|function| (function.name.clone(), function.clone()))
            .collect::<HashMap<_, _>>(),
    );
    let methods = Arc::new(
        module
            .functions
            .iter()
            .filter_map(|function| {
                function.receiver_type.as_ref().map(|receiver_type| {
                    (
                        (receiver_type.name.clone(), function.name.clone()),
                        function.clone(),
                    )
                })
            })
            .collect::<HashMap<_, _>>(),
    );
    let structs = Arc::new(
        module
            .structs
            .iter()
            .map(|decl| (decl.name.clone(), decl.clone()))
            .collect::<HashMap<_, _>>(),
    );
    let enums = Arc::new(
        module
            .enums
            .iter()
            .map(|decl| (decl.name.clone(), decl.clone()))
            .collect::<HashMap<_, _>>(),
    );
    let output = OutputBuffer::default();

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

    let value = eval_function(&main, &[], &functions, &methods, &structs, &enums, &output)?;
    Ok(ExecutionResult {
        value,
        stdout: output.snapshot(),
    })
}

fn eval_function(
    function: &Function,
    args: &[Value],
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
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
        methods,
        structs,
        enums,
        output,
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
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    nested_scope: bool,
    source_path: &Path,
) -> Result<Option<Value>, Diagnostics> {
    if nested_scope {
        scopes.push();
    }

    for stmt in stmts {
        if let Some(value) = eval_stmt(
            stmt,
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
        )? {
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
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
) -> Result<Option<Value>, Diagnostics> {
    match stmt {
        Stmt::Return(expr, _) => Ok(Some(eval_expr(
            expr,
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
        )?)),
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

            let value = eval_expr(
                value,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
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
            let value = eval_expr(
                value,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
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
            let condition = eval_expr(
                condition,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            match condition {
                Value::Bool(true) => eval_block(
                    then_body,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    true,
                    source_path,
                ),
                Value::Bool(false) => eval_block(
                    else_body,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    true,
                    source_path,
                ),
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
                let value = eval_expr(
                    condition,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )?;
                match value {
                    Value::Bool(true) => {
                        if let Some(result) = eval_block(
                            body,
                            scopes,
                            functions,
                            methods,
                            structs,
                            enums,
                            output,
                            true,
                            source_path,
                        )? {
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
        Stmt::Match { value, arms, span } => {
            let target = eval_expr(
                value,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            let Value::Enum(target_enum) = target else {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3032",
                        "`match` currently requires an enum value",
                        format!("this match target resolves to `{}`", value_name(&target)),
                        value.span(),
                    )
                    .with_fix_it("match over a value whose type is a known enum")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            };

            let mut seen_variants = HashSet::new();
            let mut resolved_arms = Vec::new();
            for arm in arms {
                let pattern = resolve_match_pattern(
                    &arm.pattern,
                    &target_enum.name,
                    enums,
                    &mut seen_variants,
                    source_path,
                )?;
                resolved_arms.push((pattern, arm));
            }

            let missing = enums
                .get(&target_enum.name)
                .map(|decl| {
                    decl.variants
                        .iter()
                        .filter(|variant| !seen_variants.contains(&variant.name))
                        .map(|variant| variant.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !missing.is_empty() {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3033",
                        format!("non-exhaustive match over `{}`", target_enum.name),
                        format!(
                            "missing arm(s): {}",
                            missing
                                .iter()
                                .map(|variant| format!("{}.{variant}", target_enum.name))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        *span,
                    )
                    .with_fix_it("add match arms for every remaining enum variant")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            }

            for (pattern, arm) in resolved_arms {
                if pattern == target_enum {
                    return eval_block(
                        &arm.body,
                        scopes,
                        functions,
                        methods,
                        structs,
                        enums,
                        output,
                        true,
                        source_path,
                    );
                }
            }

            Ok(None)
        }
        Stmt::Select { arms, span } => loop {
            if arms.is_empty() {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` requires at least one arm",
                        "the bootstrap select model needs one or more `recv(channel)` arms",
                        *span,
                    )
                    .with_fix_it("add at least one select arm like `value = recv(ch):`")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            }

            for arm in arms {
                if let Some(received) = try_eval_select_operation(
                    &arm.operation,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )? {
                    scopes.push();
                    if let Some(binding) = &arm.binding {
                        scopes.define_current(
                            binding.clone(),
                            Binding {
                                mutable: false,
                                value: received,
                            },
                        );
                    }
                    let result = eval_block(
                        &arm.body,
                        scopes,
                        functions,
                        methods,
                        structs,
                        enums,
                        output,
                        false,
                        source_path,
                    )?;
                    scopes.pop();
                    return Ok(result);
                }
            }

            std::thread::yield_now();
        },
        Stmt::Expr(expr, _) => {
            let _ = eval_expr(
                expr,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            Ok(None)
        }
    }
}

fn eval_expr(
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    match expr {
        Expr::Int(value, _) => Ok(Value::Int(*value)),
        Expr::String(value, _) => Ok(Value::String(value.clone())),
        Expr::Bool(value, _) => Ok(Value::Bool(*value)),
        Expr::List { items, .. } => {
            let values = items
                .iter()
                .map(|item| {
                    eval_expr(item, scopes, functions, methods, structs, enums, output, source_path)
                })
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
                return eval_len_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "print" {
                return eval_print_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "append" {
                return eval_append_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "contains" {
                return eval_contains_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "assert" {
                return eval_assert_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "read_file" {
                return eval_read_file_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "write_file" {
                return eval_write_file_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "dict" {
                return eval_dict_builtin(args, source_path, *span);
            }

            if callee == "insert" {
                return eval_insert_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "channel" {
                return eval_channel_builtin(args, source_path, *span);
            }

            if callee == "send" {
                return eval_send_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if callee == "recv" {
                return eval_recv_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                );
            }

            if let Some(decl) = structs.get(callee) {
                let values = args
                    .iter()
                    .map(|arg| {
                        eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return eval_struct_constructor(decl, values, *span, source_path);
            }

            if enums.contains_key(callee) {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3004",
                        format!("enum `{callee}` is not callable"),
                        "unit enum variants use `EnumName.Variant`, not constructor calls",
                        *span,
                    )
                    .with_fix_it("replace this call with `EnumName.Variant`")
                    .with_source_path(source_path.to_path_buf()),
                ]));
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
                .map(|arg| {
                    eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                })
                .collect::<Result<Vec<_>, _>>()?;
            eval_function(&function, &values, functions, methods, structs, enums, output)
        }
        Expr::Field {
            target,
            field,
            span,
        } => {
            if let Expr::Ident(name, _) = target.as_ref() {
                if scopes.get(name).is_none() {
                    if let Some(decl) = enums.get(name) {
                        return eval_enum_variant(decl, field, *span, source_path);
                    }
                }
            }
            let target =
                eval_expr(target, scopes, functions, methods, structs, enums, output, source_path)?;
            eval_field_access(target, field, *span, source_path)
        }
        Expr::MethodCall {
            target,
            method,
            args,
            span,
        } => {
            let receiver =
                eval_expr(target, scopes, functions, methods, structs, enums, output, source_path)?;
            let struct_name = match &receiver {
                Value::Struct(value) => value.name.clone(),
                other => {
                    return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3037",
                            format!("method call `{method}` requires a struct receiver"),
                            format!("this target resolves to `{}`", value_name(other)),
                            target.span(),
                        )
                        .with_fix_it("call methods only on struct values")
                        .with_source_path(source_path.to_path_buf()),
                    ]));
                }
            };
            let function = methods
                .get(&(struct_name.clone(), method.clone()))
                .cloned()
                .ok_or_else(|| {
                    Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3036",
                            format!("unknown method `{method}` on `{struct_name}`"),
                            "method calls currently resolve only to receiver methods declared as `fn TypeName.method(...)`",
                            *span,
                        )
                        .with_fix_it("declare the method on the struct or call an existing method name")
                        .with_source_path(source_path.to_path_buf()),
                    ])
                })?;
            let mut values = vec![receiver];
            values.extend(
                args.iter()
                    .map(|arg| {
                        eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            eval_function(&function, &values, functions, methods, structs, enums, output)
        }
        Expr::Index {
            target,
            index,
            span,
        } => {
            let target =
                eval_expr(target, scopes, functions, methods, structs, enums, output, source_path)?;
            let index =
                eval_expr(index, scopes, functions, methods, structs, enums, output, source_path)?;
            eval_index(target, index, *span, source_path)
        }
        Expr::Go { value, span } => match value.as_ref() {
            Expr::Call { callee, args, span } => {
                if structs.contains_key(callee) {
                    return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3008",
                            "`go` currently requires a named function call",
                            "the bootstrap concurrency model only supports `go some_fn(...)` for top-level functions",
                            *span,
                        )
                        .with_fix_it("replace this expression with `go some_function(...)`")
                        .with_source_path(source_path.to_path_buf()),
                    ]));
                }
                let values = args
                    .iter()
                    .map(|arg| {
                        eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                spawn_task(
                    callee,
                    values,
                    *span,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )
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
        Expr::Await { value, span } => {
            match eval_expr(value, scopes, functions, methods, structs, enums, output, source_path)? {
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
            }
        }
        Expr::Unary { op, value, span } => {
            let value =
                eval_expr(value, scopes, functions, methods, structs, enums, output, source_path)?;
            eval_unary(*op, value, *span, source_path)
        }
        Expr::Binary { lhs, op, rhs, span } => {
            if matches!(op, BinaryOp::And | BinaryOp::Or) {
                let lhs =
                    eval_expr(lhs, scopes, functions, methods, structs, enums, output, source_path)?;
                return eval_logical(
                    lhs,
                    *op,
                    rhs,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    *span,
                    source_path,
                );
            }

            let lhs =
                eval_expr(lhs, scopes, functions, methods, structs, enums, output, source_path)?;
            let rhs =
                eval_expr(rhs, scopes, functions, methods, structs, enums, output, source_path)?;
            eval_binary(lhs, *op, rhs, *span, source_path)
        }
    }
}

fn eval_unary(
    op: UnaryOp,
    value: Value,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    match (op, value) {
        (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
        (UnaryOp::Not, other) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3026",
                "`not` requires a `bool` operand",
                format!("this operand resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("apply `not` only to boolean expressions")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn eval_logical(
    lhs: Value,
    op: BinaryOp,
    rhs: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    let Value::Bool(lhs) = lhs else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3025",
                "logical operators require `bool` operands",
                "the left operand must resolve to `bool`",
                span,
            )
            .with_fix_it("use `and` and `or` only with boolean expressions")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    match (op, lhs) {
        (BinaryOp::And, false) => Ok(Value::Bool(false)),
        (BinaryOp::Or, true) => Ok(Value::Bool(true)),
        _ => {
            let rhs = eval_expr(
                rhs,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            match rhs {
                Value::Bool(rhs) => Ok(Value::Bool(match op {
                    BinaryOp::And => lhs && rhs,
                    BinaryOp::Or => lhs || rhs,
                    _ => unreachable!("eval_logical only handles `and` and `or`"),
                })),
                other => Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3025",
                        "logical operators require `bool` operands",
                        format!("the right operand resolves to `{}`", value_name(&other)),
                        span,
                    )
                    .with_fix_it("use `and` and `or` only with boolean expressions")
                    .with_source_path(source_path.to_path_buf()),
                ])),
            }
        }
    }
}

fn spawn_task(
    callee: &str,
    args: Vec<Value>,
    span: Span,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
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
    let methods = Arc::clone(methods);
    let structs = Arc::clone(structs);
    let enums = Arc::clone(enums);
    let output = output.clone();

    std::thread::spawn(move || {
        let result = match std::panic::catch_unwind(AssertUnwindSafe(|| {
            eval_function(
                &function, &args, &functions, &methods, &structs, &enums, &output,
            )
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

fn eval_struct_constructor(
    decl: &StructDecl,
    args: Vec<Value>,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    if decl.fields.len() != args.len() {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{}`", decl.name),
                format!(
                    "expected {} field value(s), got {}",
                    decl.fields.len(),
                    args.len()
                ),
                span,
            )
            .with_fix_it("pass one argument for each struct field in declaration order")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::Struct(StructValue {
        name: decl.name.clone(),
        fields: decl
            .fields
            .iter()
            .map(|field| field.name.clone())
            .zip(args)
            .collect(),
    }))
}

fn eval_enum_variant(
    decl: &EnumDecl,
    variant: &str,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    if decl
        .variants
        .iter()
        .any(|candidate| candidate.name == variant)
    {
        Ok(Value::Enum(EnumValue {
            name: decl.name.clone(),
            variant: variant.to_string(),
        }))
    } else {
        Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3028",
                format!("unknown variant `{variant}` on `{}`", decl.name),
                "enum variant references must use a variant declared on the enum",
                span,
            )
            .with_fix_it("use one of the variants declared on the enum")
            .with_source_path(source_path.to_path_buf()),
        ]))
    }
}

fn resolve_match_pattern(
    pattern: &Expr,
    expected_enum: &str,
    enums: &EnumTable,
    seen_variants: &mut HashSet<String>,
    source_path: &Path,
) -> Result<EnumValue, Diagnostics> {
    let Expr::Field {
        target,
        field,
        span,
    } = pattern
    else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3031",
                "match arms must use enum variants",
                "each match arm pattern must be written as `EnumName.Variant`",
                pattern.span(),
            )
            .with_fix_it("replace this pattern with a unit enum variant like `Status.Ready`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let Expr::Ident(enum_name, _) = target.as_ref() else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3031",
                "match arms must use enum variants",
                "each match arm pattern must be written as `EnumName.Variant`",
                pattern.span(),
            )
            .with_fix_it("replace this pattern with a unit enum variant like `Status.Ready`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if enum_name != expected_enum {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3031",
                "match arm uses a variant from a different enum",
                format!(
                    "this match targets `{expected_enum}`, but the arm pattern belongs to `{enum_name}`"
                ),
                *span,
            )
            .with_fix_it(format!("use a `{expected_enum}.Variant` pattern here"))
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let decl = enums.get(enum_name).ok_or_else(|| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3031",
                "match arm uses an unknown enum",
                format!("`{enum_name}` is not a declared enum in this module graph"),
                *span,
            )
            .with_fix_it("use a declared enum name in the arm pattern")
            .with_source_path(source_path.to_path_buf()),
        ])
    })?;
    let value = match eval_enum_variant(decl, field, *span, source_path)? {
        Value::Enum(value) => value,
        _ => unreachable!("enum variant evaluation should always yield an enum value"),
    };

    if !seen_variants.insert(value.variant.clone()) {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3030",
                format!(
                    "duplicate match arm for `{expected_enum}.{}`",
                    value.variant
                ),
                "each unit enum variant can appear only once in a match over the same enum",
                *span,
            )
            .with_fix_it("remove the duplicate arm or replace it with another enum variant")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(value)
}

fn eval_field_access(
    target: Value,
    field: &str,
    span: Span,
    source_path: &Path,
) -> Result<Value, Diagnostics> {
    match target {
        Value::Struct(value) => value.field(field).cloned().ok_or_else(|| {
            Diagnostics(vec![
                Diagnostic::error(
                    "GOF3022",
                    format!("unknown field `{field}` on `{}`", value.name),
                    "field access must reference a field declared on the target struct",
                    span,
                )
                .with_fix_it("use one of the fields declared on the struct")
                .with_source_path(source_path.to_path_buf()),
            ])
        }),
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3024",
                "field access requires a struct value",
                format!("this target resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("access fields only on struct values")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
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
        (Value::Struct(lhs), BinaryOp::Eq, Value::Struct(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Struct(lhs), BinaryOp::Ne, Value::Struct(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Enum(lhs), BinaryOp::Eq, Value::Enum(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Enum(lhs), BinaryOp::Ne, Value::Enum(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::List(lhs), BinaryOp::Eq, Value::List(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::List(lhs), BinaryOp::Ne, Value::List(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Dict(lhs), BinaryOp::Eq, Value::Dict(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Dict(lhs), BinaryOp::Ne, Value::Dict(rhs)) => Ok(Value::Bool(lhs != rhs)),
        _ => Err(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "the current evaluator supports int arithmetic, comparisons, and equality for strings, bools, lists, dicts, structs, and enums",
            span,
        )
        .with_source_path(source_path.to_path_buf())])),
    }
}

fn eval_len_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
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

    let value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    match value {
        Value::List(values) => Ok(Value::Int(values.len() as i64)),
        Value::Dict(value) => Ok(Value::Int(value.len() as i64)),
        Value::String(value) => Ok(Value::Int(value.chars().count() as i64)),
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3019",
                "`len` requires a list, dict, or string value",
                format!("this argument resolves to `{}`", value_name(&other)),
                args[0].span(),
            )
            .with_fix_it("pass a list, dict, or string value to `len`")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn eval_print_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 1 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `print`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `print` with exactly one printable value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Some(rendered) = value.printable_text() else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3038",
                "`print` requires a printable value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("print ints, strings, bools, lists, dicts, structs, or enums instead")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    output.push_line(&rendered);
    Ok(Value::Unit)
}

fn eval_append_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 2 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `append`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `append` as `append(list_value, item)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let list_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let appended = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    match list_value {
        Value::List(mut values) => {
            values.push(appended);
            Ok(Value::List(values))
        }
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3039",
                "`append` requires a list as its first argument",
                format!("this argument resolves to `{}`", value_name(&other)),
                args[0].span(),
            )
            .with_fix_it("pass a list value as the first argument to `append`")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn eval_contains_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 2 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `contains`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `contains` as `contains(haystack, needle)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let haystack = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let needle = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    match (haystack, needle) {
        (Value::String(haystack), Value::String(needle)) => {
            Ok(Value::Bool(haystack.contains(&needle)))
        }
        (Value::List(values), needle) => {
            Ok(Value::Bool(values.iter().any(|value| value == &needle)))
        }
        (Value::Dict(values), Value::String(key)) => Ok(Value::Bool(values.contains_key(&key))),
        (Value::String(_), needle) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string needle for string haystacks",
                format!("this needle resolves to `{}`", value_name(&needle)),
                args[1].span(),
            )
            .with_fix_it("pass a string as the second argument to `contains`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (Value::Dict(_), needle) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string key for dict haystacks",
                format!("this needle resolves to `{}`", value_name(&needle)),
                args[1].span(),
            )
            .with_fix_it("pass a string key as the second argument to `contains`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (other, _) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string, list, or dict haystack",
                format!("this haystack resolves to `{}`", value_name(&other)),
                args[0].span(),
            )
            .with_fix_it("call `contains` with a string, list, or dict as the first argument")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn eval_assert_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if !(1..=2).contains(&args.len()) {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `assert`",
                format!("expected 1 or 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `assert(condition)` or `assert(condition, \"message\")`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let condition = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let message = if args.len() == 2 {
        Some(eval_expr(
            &args[1],
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
        )?)
    } else {
        None
    };

    let Value::Bool(condition) = condition else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3041",
                "`assert` requires a boolean condition",
                format!("this condition resolves to `{}`", value_name(&condition)),
                args[0].span(),
            )
            .with_fix_it("pass a boolean expression as the first argument to `assert`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if let Some(message) = &message {
        if !matches!(message, Value::String(_)) {
            return Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3041",
                    "`assert` requires a string message when a second argument is present",
                    format!("this message resolves to `{}`", value_name(message)),
                    args[1].span(),
                )
                .with_fix_it("pass a string as the second argument to `assert`")
                .with_source_path(source_path.to_path_buf()),
            ]));
        }
    }

    if condition {
        Ok(Value::Unit)
    } else {
        let detail = match message {
            Some(Value::String(text)) => text,
            _ => "assertion failed".to_string(),
        };
        Err(Diagnostics(vec![
            Diagnostic::error("GOF3042", "assertion failed", detail, args[0].span())
                .with_fix_it("adjust the asserted condition or the preceding logic")
                .with_source_path(source_path.to_path_buf()),
        ]))
    }
}

fn eval_read_file_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 1 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `read_file`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_file(path)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::String(path_text) = path_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3043",
                "`read_file` requires a string path",
                format!("this path resolves to `{}`", value_name(&path_value)),
                args[0].span(),
            )
            .with_fix_it("pass a string path like `\"notes.txt\"` to `read_file`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    fs::read_to_string(&path_text)
        .map(Value::String)
        .map_err(|error| {
            Diagnostics(vec![
                Diagnostic::error(
                    "GOF3044",
                    format!("failed to read file `{path_text}`"),
                    error.to_string(),
                    args[0].span(),
                )
                .with_fix_it("ensure the file exists and is readable")
                .with_source_path(source_path.to_path_buf()),
            ])
        })
}

fn eval_write_file_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 2 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `write_file`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `write_file(path, contents)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let contents_value = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::String(path_text) = path_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3043",
                "`write_file` requires a string path",
                format!("this path resolves to `{}`", value_name(&path_value)),
                args[0].span(),
            )
            .with_fix_it("pass a string path as the first argument to `write_file`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::String(contents) = contents_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3043",
                "`write_file` requires string contents",
                format!(
                    "this contents value resolves to `{}`",
                    value_name(&contents_value)
                ),
                args[1].span(),
            )
            .with_fix_it("pass a string as the second argument to `write_file`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    fs::write(&path_text, contents).map_err(|error| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3044",
                format!("failed to write file `{path_text}`"),
                error.to_string(),
                args[0].span(),
            )
            .with_fix_it("ensure the target path is writable")
            .with_source_path(source_path.to_path_buf()),
        ])
    })?;
    Ok(Value::Unit)
}

fn eval_dict_builtin(args: &[Expr], source_path: &Path, span: Span) -> Result<Value, Diagnostics> {
    if !args.is_empty() {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `dict`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `dict()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::Dict(DictValue::new()))
}

fn eval_insert_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 3 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `insert`",
                format!("expected 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `insert(dict_value, \"key\", value)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let dict_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let key_value = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let inserted = eval_expr(
        &args[2],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::Dict(dict_value) = dict_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3045",
                "`insert` requires a dict as its first argument",
                format!("this argument resolves to `{}`", value_name(&dict_value)),
                args[0].span(),
            )
            .with_fix_it("pass a dict value as the first argument to `insert`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::String(key) = key_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3045",
                "`insert` requires a string key",
                format!("this key resolves to `{}`", value_name(&key_value)),
                args[1].span(),
            )
            .with_fix_it("pass a string as the second argument to `insert`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(Value::Dict(dict_value.insert(key, inserted)))
}

fn eval_channel_builtin(
    args: &[Expr],
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if !args.is_empty() {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `channel`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `channel()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }
    Ok(Value::Channel(ChannelValue(Arc::new(ChannelHandle::new()))))
}

fn eval_send_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 2 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `send`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `send(channel_value, item)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let channel_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let sent_value = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::Channel(channel_value) = channel_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3046",
                "`send` requires a channel as its first argument",
                format!("this argument resolves to `{}`", value_name(&channel_value)),
                args[0].span(),
            )
            .with_fix_it("pass a channel value as the first argument to `send`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    channel_value.send(sent_value, args[0].span(), source_path)?;
    Ok(Value::Unit)
}

fn eval_recv_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> Result<Value, Diagnostics> {
    if args.len() != 1 {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `recv`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `recv(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let channel_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::Channel(channel_value) = channel_value else {
        return Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3046",
                "`recv` requires a channel value",
                format!("this argument resolves to `{}`", value_name(&channel_value)),
                args[0].span(),
            )
            .with_fix_it("pass a channel value to `recv`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    channel_value.recv(args[0].span(), source_path)
}

fn try_eval_select_operation(
    operation: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
) -> Result<Option<Value>, Diagnostics> {
    match operation {
        Expr::Call { callee, args, span } if callee == "recv" => {
            if args.len() != 1 {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` arms currently require `recv(channel)` operations",
                        "each select arm must call `recv` with exactly one channel argument",
                        *span,
                    )
                    .with_fix_it("rewrite the arm as `recv(channel):` or `value = recv(channel):`")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            }

            let channel_value = eval_expr(
                &args[0],
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            let Value::Channel(channel_value) = channel_value else {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` arms require channel receives",
                        format!(
                            "this arm resolves to `{}` instead of a channel",
                            value_name(&channel_value)
                        ),
                        args[0].span(),
                    )
                    .with_fix_it("pass a channel value to `recv` inside the select arm")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            };

            channel_value.try_recv(args[0].span(), source_path)
        }
        Expr::Call { callee, .. } => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(channel)` operations",
                format!("this arm uses `{callee}(...)` instead"),
                operation.span(),
            )
            .with_fix_it("replace the arm operation with `recv(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        _ => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(channel)` operations",
                "select arms must be written as `recv(channel):` or `value = recv(channel):`",
                operation.span(),
            )
            .with_fix_it("replace this arm with a `recv(channel)` operation")
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
    match (target, index) {
        (Value::List(values), Value::Int(index)) => {
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

            values.get(index as usize).cloned().ok_or_else(|| {
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
            })
        }
        (Value::List(_), index) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "list indexing requires an `int` index",
                format!("this index resolves to `{}`", value_name(&index)),
                span,
            )
            .with_fix_it("use an integer index like `values[0]`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (Value::Dict(values), Value::String(key)) => values.get(&key).cloned().ok_or_else(|| {
            Diagnostics(vec![
                Diagnostic::error(
                    "GOF3018",
                    format!("dict key `{key}` does not exist"),
                    "dict indexing currently requires an existing string key",
                    span,
                )
                .with_fix_it("insert the key first or check it with `contains(dict, key)`")
                .with_source_path(source_path.to_path_buf()),
            ])
        }),
        (Value::Dict(_), index) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "dict indexing requires a `string` key",
                format!("this index resolves to `{}`", value_name(&index)),
                span,
            )
            .with_fix_it("use a string key like `values[\"name\"]`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (other, _) => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3018",
                "indexing requires a list or dict value",
                format!("this target resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("index only list or dict values")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn value_name(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "int",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::Struct(_) => "struct",
        Value::Enum(_) => "enum",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Channel(_) => "channel",
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
    use super::{Value, run, run_with_output};
    use crate::ast::parse;
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;
    use tempfile::tempdir;

    fn run_source(text: &str) -> Result<Value, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("test.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        run(&module)
    }

    fn run_source_with_output(
        text: &str,
    ) -> Result<super::ExecutionResult, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("test.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        run_with_output(&module)
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

    #[test]
    fn evaluates_append_and_contains() {
        let value = run_source(
            "fn main() -> int:\n    values = append([2, 4], 6)\n    if contains(values, 6) and contains(\"gof-lang\", \"lang\"):\n        return len(values) + values[2]\n    return 0\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(9));
    }

    #[test]
    fn evaluates_dict_assert_and_file_io() {
        let temp = tempdir().expect("tempdir should exist");
        let input_path = temp.path().join("in.txt");
        let output_path = temp.path().join("out.txt");
        std::fs::write(&input_path, "gof").expect("input file should be written");
        let input = input_path.to_string_lossy().replace('\\', "\\\\");
        let output = output_path.to_string_lossy().replace('\\', "\\\\");

        let value = run_source(&format!(
            "fn main() -> int:\n    path = \"{input}\"\n    out = \"{output}\"\n    mut data: dict = dict()\n    data = insert(data, \"size\", len(read_file(path)))\n    assert(contains(data, \"size\"), \"missing size\")\n    write_file(out, read_file(path))\n    return data[\"size\"]\n"
        ))
        .expect("program should run");

        assert_eq!(value, Value::Int(3));
        assert_eq!(
            std::fs::read_to_string(&output_path).expect("output file should exist"),
            "gof"
        );
    }

    #[test]
    fn evaluates_channels_and_select() {
        let value = run_source(
            "fn main() -> int:\n    left: channel = channel()\n    right: channel = channel()\n    send(right, 8)\n    select:\n        value = recv(left):\n            return 0\n        value = recv(right):\n            return value + 1\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(9));
    }

    #[test]
    fn rejects_failed_assertions() {
        let diagnostics = run_source("fn main() -> unit:\n    assert(false, \"boom\")\n")
            .expect_err("assert should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3042"]);
    }

    #[test]
    fn evaluates_struct_construction_and_field_access() {
        let value = run_source(
            "struct Point:\n    x: int\n    y: int\n\nfn total(point: Point) -> int:\n    return point.x + point.y\n\nfn main() -> int:\n    point = Point(8, 13)\n    return total(point)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(21));
    }

    #[test]
    fn evaluates_logical_operators() {
        let value = run_source("fn main() -> bool:\n    return not false and true or false\n")
            .expect("program should run");
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn evaluates_enum_variant_equality() {
        let value = run_source(
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> bool:\n    return Status.Ready != Status.Busy\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn evaluates_match_over_enum_variants() {
        let value = run_source(
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 1\n        Status.Busy:\n            return 2\n\nfn main() -> int:\n    return score(Status.Busy)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(2));
    }

    #[test]
    fn evaluates_receiver_methods() {
        let value = run_source(
            "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(12));
    }

    #[test]
    fn captures_print_output() {
        let result = run_source_with_output(
            "fn main() -> int:\n    print(\"gof\")\n    print(42)\n    return 7\n",
        )
        .expect("program should run");
        assert_eq!(result.stdout, "gof\n42\n");
        assert_eq!(result.value, Value::Int(7));
    }

    #[test]
    fn rejects_non_exhaustive_match() {
        let diagnostics = run_source(
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> int:\n    current = Status.Ready\n    match current:\n        Status.Ready:\n            return 1\n",
        )
        .expect_err("non-exhaustive match should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3033"]);
    }

    #[test]
    fn rejects_unknown_enum_variant() {
        let diagnostics = run_source(
            "enum Status:\n    Ready\n\nfn main() -> bool:\n    return Status.Busy == Status.Ready\n",
        )
        .expect_err("unknown enum variant should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3028"]);
    }
}
