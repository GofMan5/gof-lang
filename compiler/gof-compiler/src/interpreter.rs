use crate::ast::{
    BinaryOp, EnumDecl, EnumVariant, Expr, Function, MatchPattern, Module, Param, Stmt, StructDecl,
    UnaryOp,
};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use serde_json::Value as SerdeJsonValue;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::{Debug, Formatter};
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

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
    Json(JsonValue),
    Struct(StructValue),
    Enum(EnumValue),
    List(Vec<Value>),
    Dict(DictValue),
    Channel(ChannelValue),
    CancelToken(CancelTokenValue),
    Task(TaskValue),
    Unit,
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => f.debug_tuple("Int").field(value).finish(),
            Self::String(value) => f.debug_tuple("String").field(value).finish(),
            Self::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
            Self::Json(value) => f.debug_tuple("Json").field(value).finish(),
            Self::Struct(value) => f.debug_tuple("Struct").field(value).finish(),
            Self::Enum(value) => f.debug_tuple("Enum").field(value).finish(),
            Self::List(values) => f.debug_tuple("List").field(values).finish(),
            Self::Dict(value) => f.debug_tuple("Dict").field(value).finish(),
            Self::Channel(_) => f.write_str("Channel(<open>)"),
            Self::CancelToken(_) => f.write_str("CancelToken(<active-or-cancelled>)"),
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
            (Self::Json(lhs), Self::Json(rhs)) => lhs == rhs,
            (Self::Struct(lhs), Self::Struct(rhs)) => lhs == rhs,
            (Self::Enum(lhs), Self::Enum(rhs)) => lhs == rhs,
            (Self::List(lhs), Self::List(rhs)) => lhs == rhs,
            (Self::Dict(lhs), Self::Dict(rhs)) => lhs == rhs,
            (Self::Channel(lhs), Self::Channel(rhs)) => lhs.ptr_eq(rhs),
            (Self::CancelToken(lhs), Self::CancelToken(rhs)) => lhs.ptr_eq(rhs),
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
            Self::Json(value) => Some(value.cli_text()),
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
            Self::CancelToken(_) => Some("<cancel_token>".to_string()),
            Self::Task(_) => Some("<task>".to_string()),
            Self::Unit => None,
        }
    }

    fn printable_text(&self) -> Option<String> {
        match self {
            Self::Channel(_) | Self::CancelToken(_) | Self::Task(_) | Self::Unit => None,
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

    fn keys_list(&self) -> Vec<Value> {
        self.entries.keys().cloned().map(Value::String).collect()
    }

    fn values_list(&self) -> Vec<Value> {
        self.entries.values().cloned().collect()
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
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    fn cli_text(&self) -> String {
        serde_json::to_string(&self.to_serde()).expect("json rendering should not fail")
    }

    fn to_serde(&self) -> SerdeJsonValue {
        match self {
            Self::Null => SerdeJsonValue::Null,
            Self::Bool(value) => SerdeJsonValue::Bool(*value),
            Self::Int(value) => SerdeJsonValue::Number((*value).into()),
            Self::String(value) => SerdeJsonValue::String(value.clone()),
            Self::Array(items) => {
                SerdeJsonValue::Array(items.iter().map(JsonValue::to_serde).collect())
            }
            Self::Object(entries) => SerdeJsonValue::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_serde()))
                    .collect(),
            ),
        }
    }
}

fn json_from_serde(value: SerdeJsonValue) -> Result<JsonValue, String> {
    match value {
        SerdeJsonValue::Null => Ok(JsonValue::Null),
        SerdeJsonValue::Bool(value) => Ok(JsonValue::Bool(value)),
        SerdeJsonValue::Number(number) => number.as_i64().map(JsonValue::Int).ok_or_else(|| {
            "only integer JSON numbers are supported in the bootstrap runtime".to_string()
        }),
        SerdeJsonValue::String(value) => Ok(JsonValue::String(value)),
        SerdeJsonValue::Array(items) => items
            .into_iter()
            .map(json_from_serde)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        SerdeJsonValue::Object(entries) => entries
            .into_iter()
            .map(|(key, value)| json_from_serde(value).map(|json| (key, json)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(JsonValue::Object),
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
    payloads: Vec<(String, Value)>,
}

#[derive(Clone)]
pub struct CancelTokenValue(Arc<AtomicBool>);

impl CancelTokenValue {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Debug for CancelTokenValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelTokenValue")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl EnumValue {
    fn cli_text(&self) -> String {
        if self.payloads.is_empty() {
            format!("{}.{}", self.name, self.variant)
        } else {
            format!(
                "{}.{}({})",
                self.name,
                self.variant,
                self.payloads
                    .iter()
                    .map(|(name, value)| format!(
                        "{name}: {}",
                        value.cli_text().unwrap_or_else(|| "unit".to_string())
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

fn builtin_enum_table() -> HashMap<String, EnumDecl> {
    let builtin_path = PathBuf::from("<builtin>");
    HashMap::from([(
        "RuntimeError".to_string(),
        EnumDecl {
            name: "RuntimeError".to_string(),
            variants: vec![
                EnumVariant {
                    name: "EnvMissing".to_string(),
                    fields: vec![crate::ast::EnumVariantField {
                        name: "name".to_string(),
                        ty: crate::ast::TypeRef {
                            name: "string".to_string(),
                            args: Vec::new(),
                            span: Span::new(0, 0, 0),
                        },
                        span: Span::new(0, 0, 0),
                    }],
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "Io".to_string(),
                    fields: vec![crate::ast::EnumVariantField {
                        name: "message".to_string(),
                        ty: crate::ast::TypeRef {
                            name: "string".to_string(),
                            args: Vec::new(),
                            span: Span::new(0, 0, 0),
                        },
                        span: Span::new(0, 0, 0),
                    }],
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "ChannelClosed".to_string(),
                    fields: Vec::new(),
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "Cancelled".to_string(),
                    fields: Vec::new(),
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "Json".to_string(),
                    fields: vec![crate::ast::EnumVariantField {
                        name: "message".to_string(),
                        ty: crate::ast::TypeRef {
                            name: "string".to_string(),
                            args: Vec::new(),
                            span: Span::new(0, 0, 0),
                        },
                        span: Span::new(0, 0, 0),
                    }],
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "HttpRequest".to_string(),
                    fields: vec![crate::ast::EnumVariantField {
                        name: "message".to_string(),
                        ty: crate::ast::TypeRef {
                            name: "string".to_string(),
                            args: Vec::new(),
                            span: Span::new(0, 0, 0),
                        },
                        span: Span::new(0, 0, 0),
                    }],
                    span: Span::new(0, 0, 0),
                },
                EnumVariant {
                    name: "HttpStatus".to_string(),
                    fields: vec![
                        crate::ast::EnumVariantField {
                            name: "code".to_string(),
                            ty: crate::ast::TypeRef {
                                name: "int".to_string(),
                                args: Vec::new(),
                                span: Span::new(0, 0, 0),
                            },
                            span: Span::new(0, 0, 0),
                        },
                        crate::ast::EnumVariantField {
                            name: "body".to_string(),
                            ty: crate::ast::TypeRef {
                                name: "string".to_string(),
                                args: Vec::new(),
                                span: Span::new(0, 0, 0),
                            },
                            span: Span::new(0, 0, 0),
                        },
                    ],
                    span: Span::new(0, 0, 0),
                },
            ],
            span: Span::new(0, 0, 0),
            source_path: builtin_path,
        },
    )])
}

#[derive(Debug, Clone)]
struct ResolvedMatchPattern {
    enum_name: String,
    variant: String,
    bindings: Vec<String>,
}

#[derive(Clone)]
pub struct ChannelValue(Arc<ChannelHandle>);

impl ChannelValue {
    fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn close(&self) {
        self.0.close();
    }

    fn send(&self, value: Value, token: Option<&CancelTokenValue>) -> ChannelReceiveState {
        self.0.send(value, token)
    }

    fn recv(&self, token: Option<&CancelTokenValue>) -> ChannelReceiveState {
        self.0.recv(token)
    }

    fn try_recv(&self, token: Option<&CancelTokenValue>) -> Option<ChannelReceiveState> {
        self.0.try_recv(token)
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
struct ChannelState {
    queue: VecDeque<Value>,
    closed: bool,
}

#[derive(Debug)]
struct ChannelHandle {
    state: Mutex<ChannelState>,
    ready: Condvar,
}

#[derive(Debug, Clone)]
enum ChannelReceiveState {
    Value(Value),
    Closed,
    Cancelled,
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
        Self {
            state: Mutex::new(ChannelState {
                queue: VecDeque::new(),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn close(&self) {
        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        state.closed = true;
        self.ready.notify_all();
    }

    fn send(&self, value: Value, token: Option<&CancelTokenValue>) -> ChannelReceiveState {
        if token.is_some_and(CancelTokenValue::is_cancelled) {
            return ChannelReceiveState::Cancelled;
        }

        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        if state.closed {
            return ChannelReceiveState::Closed;
        }
        state.queue.push_back(value);
        self.ready.notify_one();
        ChannelReceiveState::Value(Value::Unit)
    }

    fn recv(&self, token: Option<&CancelTokenValue>) -> ChannelReceiveState {
        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        loop {
            if let Some(value) = state.queue.pop_front() {
                return ChannelReceiveState::Value(value);
            }
            if state.closed {
                return ChannelReceiveState::Closed;
            }
            if token.is_some_and(CancelTokenValue::is_cancelled) {
                return ChannelReceiveState::Cancelled;
            }
            let (next_state, _) = self
                .ready
                .wait_timeout(state, Duration::from_millis(10))
                .expect("channel wait should not be poisoned");
            state = next_state;
        }
    }

    fn try_recv(&self, token: Option<&CancelTokenValue>) -> Option<ChannelReceiveState> {
        if token.is_some_and(CancelTokenValue::is_cancelled) {
            return Some(ChannelReceiveState::Cancelled);
        }

        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        if let Some(value) = state.queue.pop_front() {
            Some(ChannelReceiveState::Value(value))
        } else if state.closed {
            Some(ChannelReceiveState::Closed)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
struct Binding {
    mutable: bool,
    value: Value,
}

enum EvalOutcome {
    Next,
    Return(Value),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
enum EvalSignal {
    Diagnostics(Diagnostics),
    Propagate(Value),
}

type EvalResult<T> = std::result::Result<T, EvalSignal>;

impl From<Diagnostics> for EvalSignal {
    fn from(value: Diagnostics) -> Self {
        Self::Diagnostics(value)
    }
}

fn eval_diagnostics<T>(diagnostics: Diagnostics) -> EvalResult<T> {
    Err(EvalSignal::Diagnostics(diagnostics))
}

fn result_ok(value: Value) -> Value {
    Value::Enum(EnumValue {
        name: "Result".to_string(),
        variant: "Ok".to_string(),
        payloads: vec![("value".to_string(), value)],
    })
}

fn result_err(error: Value) -> Value {
    Value::Enum(EnumValue {
        name: "Result".to_string(),
        variant: "Err".to_string(),
        payloads: vec![("error".to_string(), error)],
    })
}

fn enum_value(name: &str, variant: &str, payloads: Vec<(String, Value)>) -> Value {
    Value::Enum(EnumValue {
        name: name.to_string(),
        variant: variant.to_string(),
        payloads,
    })
}

fn runtime_io_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Io",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_env_missing_error(name: &str) -> Value {
    enum_value(
        "RuntimeError",
        "EnvMissing",
        vec![("name".to_string(), Value::String(name.to_string()))],
    )
}

fn runtime_channel_closed_error() -> Value {
    enum_value("RuntimeError", "ChannelClosed", Vec::new())
}

fn runtime_cancelled_error() -> Value {
    enum_value("RuntimeError", "Cancelled", Vec::new())
}

fn runtime_json_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Json",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_http_request_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "HttpRequest",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_http_status_error(code: i64, body: String) -> Value {
    enum_value(
        "RuntimeError",
        "HttpStatus",
        vec![
            ("code".to_string(), Value::Int(code)),
            ("body".to_string(), Value::String(body)),
        ],
    )
}

fn host_program_args() -> Vec<String> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() <= 1 {
        return Vec::new();
    }

    if args.len() >= 3 && args[1] == "run" {
        let mut program_args = args.into_iter().skip(3).collect::<Vec<_>>();
        if matches!(program_args.first(), Some(first) if first == "--") {
            program_args.remove(0);
        }
        return program_args;
    }

    args.into_iter().skip(1).collect()
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
    let mut enum_table = module
        .enums
        .iter()
        .map(|decl| (decl.name.clone(), decl.clone()))
        .collect::<HashMap<_, _>>();
    enum_table.extend(builtin_enum_table());
    let enums = Arc::new(enum_table);
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
    let outcome = match eval_block(
        &function.body,
        &mut scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        0,
        false,
        &function.source_path,
    ) {
        Ok(outcome) => outcome,
        Err(EvalSignal::Diagnostics(diagnostics)) => return Err(diagnostics),
        Err(EvalSignal::Propagate(value)) => EvalOutcome::Return(value),
    };

    match outcome {
        EvalOutcome::Next => Ok(Value::Unit),
        EvalOutcome::Return(value) => Ok(value),
        EvalOutcome::Break => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3053",
                "`break` is only valid inside a loop",
                "`break` currently works only inside `while` and `for` loop bodies",
                function.span,
            )
            .with_fix_it("move this statement into a surrounding `while` or `for` loop")
            .with_source_path(function.source_path.clone()),
        ])),
        EvalOutcome::Continue => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3054",
                "`continue` is only valid inside a loop",
                "`continue` currently works only inside `while` and `for` loop bodies",
                function.span,
            )
            .with_fix_it("move this statement into a surrounding `while` or `for` loop")
            .with_source_path(function.source_path.clone()),
        ])),
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
    loop_depth: usize,
    nested_scope: bool,
    source_path: &Path,
) -> EvalResult<EvalOutcome> {
    if nested_scope {
        scopes.push();
    }

    for stmt in stmts {
        let outcome = eval_stmt(
            stmt,
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            loop_depth,
            source_path,
        )?;
        if !matches!(outcome, EvalOutcome::Next) {
            if nested_scope {
                scopes.pop();
            }
            return Ok(outcome);
        }
    }

    if nested_scope {
        scopes.pop();
    }

    Ok(EvalOutcome::Next)
}

fn eval_stmt(
    stmt: &Stmt,
    scopes: &mut ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    loop_depth: usize,
    source_path: &Path,
) -> EvalResult<EvalOutcome> {
    match stmt {
        Stmt::Return(expr, _) => match eval_expr(
            expr,
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
        ) {
            Ok(value) => Ok(EvalOutcome::Return(value)),
            Err(EvalSignal::Propagate(value)) => Ok(EvalOutcome::Return(value)),
            Err(EvalSignal::Diagnostics(diagnostics)) => Err(EvalSignal::Diagnostics(diagnostics)),
        },
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
                )])
                .into());
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
            Ok(EvalOutcome::Next)
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
                    ])
                    .into());
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
            Ok(EvalOutcome::Next)
        }
        Stmt::Break(span) => {
            if loop_depth == 0 {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3053",
                        "`break` is only valid inside a loop",
                        "`break` currently works only inside `while` and `for` loop bodies",
                        *span,
                    )
                    .with_fix_it("move this statement into a surrounding `while` or `for` loop")
                    .with_source_path(source_path.to_path_buf()),
                ])
                .into());
            }
            Ok(EvalOutcome::Break)
        }
        Stmt::Continue(span) => {
            if loop_depth == 0 {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3054",
                        "`continue` is only valid inside a loop",
                        "`continue` currently works only inside `while` and `for` loop bodies",
                        *span,
                    )
                    .with_fix_it("move this statement into a surrounding `while` or `for` loop")
                    .with_source_path(source_path.to_path_buf()),
                ])
                .into());
            }
            Ok(EvalOutcome::Continue)
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
                    loop_depth,
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
                    loop_depth,
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
                ])
                .into()),
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
                        match eval_block(
                            body,
                            scopes,
                            functions,
                            methods,
                            structs,
                            enums,
                            output,
                            loop_depth + 1,
                            true,
                            source_path,
                        )? {
                            EvalOutcome::Next => {}
                            EvalOutcome::Continue => continue,
                            EvalOutcome::Break => break,
                            EvalOutcome::Return(result) => {
                                return Ok(EvalOutcome::Return(result));
                            }
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
                        ])
                        .into());
                    }
                }
            }
            Ok(EvalOutcome::Next)
        }
        Stmt::For {
            binding,
            iterable,
            body,
            span,
        } => {
            let iterable = eval_expr(
                iterable,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )?;
            let items = iter_values_from_iterable(&iterable, *span, source_path)?;

            for item in items {
                scopes.push();
                scopes.define_current(
                    binding.clone(),
                    Binding {
                        mutable: false,
                        value: item,
                    },
                );
                match eval_block(
                    body,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    loop_depth + 1,
                    false,
                    source_path,
                )? {
                    EvalOutcome::Next => {}
                    EvalOutcome::Continue => {
                        scopes.pop();
                        continue;
                    }
                    EvalOutcome::Break => {
                        scopes.pop();
                        break;
                    }
                    EvalOutcome::Return(result) => {
                        scopes.pop();
                        return Ok(EvalOutcome::Return(result));
                    }
                }
                scopes.pop();
            }

            Ok(EvalOutcome::Next)
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
                ])
                .into());
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

            let missing = if target_enum.name == "Result" {
                ["Ok".to_string(), "Err".to_string()]
                    .into_iter()
                    .filter(|variant| !seen_variants.contains(variant))
                    .collect::<Vec<_>>()
            } else {
                enums
                    .get(&target_enum.name)
                    .map(|decl| {
                        decl.variants
                            .iter()
                            .filter(|variant| !seen_variants.contains(&variant.name))
                            .map(|variant| variant.name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };

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
                ])
                .into());
            }

            for (pattern, arm) in resolved_arms {
                if pattern.enum_name == target_enum.name && pattern.variant == target_enum.variant {
                    scopes.push();
                    for (binding, (_, payload_value)) in
                        pattern.bindings.iter().zip(target_enum.payloads.iter())
                    {
                        scopes.define_current(
                            binding.clone(),
                            Binding {
                                mutable: false,
                                value: payload_value.clone(),
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
                        loop_depth,
                        false,
                        source_path,
                    );
                    scopes.pop();
                    return result;
                }
            }

            Ok(EvalOutcome::Next)
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
                ])
                .into());
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
                        loop_depth,
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
            match eval_expr(
                expr,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            ) {
                Ok(_) => {}
                Err(EvalSignal::Propagate(value)) => return Ok(EvalOutcome::Return(value)),
                Err(EvalSignal::Diagnostics(diagnostics)) => {
                    return Err(EvalSignal::Diagnostics(diagnostics));
                }
            }
            Ok(EvalOutcome::Next)
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
) -> EvalResult<Value> {
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
        Expr::Dict { entries, .. } => {
            let mut dict = DictValue::new();
            for entry in entries {
                let key = eval_expr(
                    &entry.key,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )?;
                let Value::String(key) = key else {
                    return Err(Diagnostics(vec![
                        Diagnostic::error(
                            "GOF3049",
                            "dict literal keys must resolve to `string`",
                            format!("this key resolves to `{}`", value_name(&key)),
                            entry.key.span(),
                        )
                        .with_fix_it("use a string key like `{\"name\": value}`")
                        .with_source_path(source_path.to_path_buf()),
                    ])
                    .into());
                };
                let value = eval_expr(
                    &entry.value,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )?;
                dict = dict.insert(key, value);
            }
            Ok(Value::Dict(dict))
        }
        Expr::Ident(name, span) => Ok(scopes
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
            })?),
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

            if callee == "trim" {
                return eval_trim_builtin(
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

            if callee == "split" {
                return eval_split_builtin(
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

            if callee == "join" {
                return eval_join_builtin(
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

            if callee == "starts_with" {
                return eval_starts_with_builtin(
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

            if callee == "ends_with" {
                return eval_ends_with_builtin(
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

            if callee == "parse_int" {
                return eval_parse_int_builtin(
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

            if callee == "to_string" {
                return eval_to_string_builtin(
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

            if callee == "range" {
                return eval_range_builtin(
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

            if callee == "argv" {
                return eval_argv_builtin(args, source_path, *span);
            }

            if callee == "env" {
                return eval_env_builtin(
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

            if callee == "cwd" {
                return eval_cwd_builtin(args, source_path, *span);
            }

            if callee == "exists" {
                return eval_exists_builtin(
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

            if callee == "read_dir" {
                return eval_read_dir_builtin(
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

            if callee == "mkdir" {
                return eval_mkdir_builtin(
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

            if callee == "remove_file" {
                return eval_remove_file_builtin(
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

            if callee == "path_join" {
                return eval_path_join_builtin(
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

            if callee == "path_dir" {
                return eval_path_dir_builtin(
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

            if callee == "path_base" {
                return eval_path_base_builtin(
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

            if callee == "path_ext" {
                return eval_path_ext_builtin(
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
                return Ok(eval_dict_builtin(args, source_path, *span)?);
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

            if callee == "keys" {
                return eval_keys_builtin(
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

            if callee == "values" {
                return eval_values_builtin(
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
                return Ok(eval_channel_builtin(args, source_path, *span)?);
            }

            if callee == "close" {
                return eval_close_builtin(
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

            if callee == "cancel_token" {
                return eval_cancel_token_builtin(args, source_path, *span);
            }

            if callee == "cancel" {
                return eval_cancel_builtin(
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

            if callee == "is_cancelled" {
                return eval_is_cancelled_builtin(
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

            if callee == "json_parse" {
                return eval_json_parse_builtin(
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

            if callee == "json_stringify" {
                return eval_json_stringify_builtin(
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

            if callee == "json_get" {
                return eval_json_get_builtin(
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

            if callee == "json_index" {
                return eval_json_index_builtin(
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

            if callee == "json_len" {
                return eval_json_len_builtin(
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

            if callee == "json_string" {
                return eval_json_string_builtin(
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

            if callee == "json_int" {
                return eval_json_int_builtin(
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

            if callee == "http_get" {
                return eval_http_get_builtin(
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

            if callee == "http_post" {
                return eval_http_post_builtin(
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

            if callee == "sleep" {
                return eval_sleep_builtin(
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
                ])
                .into());
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
            Ok(eval_function(&function, &values, functions, methods, structs, enums, output)?)
        }
        Expr::Field {
            target,
            field,
            span,
        } => {
            if let Expr::Ident(name, _) = target.as_ref() {
                if scopes.get(name).is_none() {
                    if name == "Result" {
                        return eval_result_variant(field, &[], *span, source_path);
                    }
                    if let Some(decl) = enums.get(name) {
                        return eval_enum_variant(decl, field, &[], *span, source_path);
                    }
                }
            }
            let target =
                eval_expr(target, scopes, functions, methods, structs, enums, output, source_path)?;
            Ok(eval_field_access(target, field, *span, source_path)?)
        }
        Expr::MethodCall {
            target,
            method,
            args,
            span,
        } => {
            if let Expr::Ident(name, _) = target.as_ref() {
                if scopes.get(name).is_none() {
                    if name == "Result" {
                        let payloads = args
                            .iter()
                            .map(|arg| {
                                eval_expr(
                                    arg,
                                    scopes,
                                    functions,
                                    methods,
                                    structs,
                                    enums,
                                    output,
                                    source_path,
                                )
                            })
                            .collect::<EvalResult<Vec<_>>>()?;
                        return eval_result_variant(method, &payloads, *span, source_path);
                    }
                    if let Some(decl) = enums.get(name) {
                        let payloads = args
                            .iter()
                            .map(|arg| {
                                eval_expr(
                                    arg,
                                    scopes,
                                    functions,
                                    methods,
                                    structs,
                                    enums,
                                    output,
                                    source_path,
                                )
                            })
                            .collect::<EvalResult<Vec<_>>>()?;
                        return eval_enum_variant(decl, method, &payloads, *span, source_path);
                    }
                }
            }

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
                    ])
                    .into());
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
                    .collect::<EvalResult<Vec<_>>>()?,
            );
            Ok(eval_function(&function, &values, functions, methods, structs, enums, output)?)
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
            Ok(eval_index(target, index, *span, source_path)?)
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
                    ])
                    .into());
                }
                let values = args
                    .iter()
                    .map(|arg| {
                        eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                    })
                    .collect::<EvalResult<Vec<_>>>()?;
                Ok(spawn_task(
                    callee,
                    values,
                    *span,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                )?)
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
            ])
            .into()),
        },
        Expr::Await { value, span } => {
            match eval_expr(value, scopes, functions, methods, structs, enums, output, source_path)? {
                Value::Task(task) => Ok(task.await_value()?),
                _ => Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3009",
                        "`await` requires a task value",
                        "only values produced by `go` can currently be awaited in the bootstrap evaluator",
                        *span,
                    )
                    .with_fix_it("store `go some_function(...)` in a binding and await that task")
                    .with_source_path(source_path.to_path_buf()),
                ])
                .into()),
            }
        }
        Expr::Propagate { value, span } => {
            let value =
                eval_expr(value, scopes, functions, methods, structs, enums, output, source_path)?;
            match value {
                Value::Enum(result) if result.name == "Result" && result.variant == "Ok" => {
                    if let Some((_, payload)) = result.payloads.first() {
                        Ok(payload.clone())
                    } else {
                        Err(Diagnostics(vec![
                            Diagnostic::error(
                                "GOF3073",
                                "wrong number of payload values for `Result.Ok`",
                                "the builtin result ok variant must carry exactly one payload value",
                                *span,
                            )
                            .with_fix_it("construct the value as `Result.Ok(some_value)`")
                            .with_source_path(source_path.to_path_buf()),
                        ])
                        .into())
                    }
                }
                Value::Enum(result) if result.name == "Result" && result.variant == "Err" => {
                    Err(EvalSignal::Propagate(Value::Enum(result)))
                }
                other => Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3074",
                        "postfix `?` requires a `Result[T, E]` operand",
                        format!("this operand resolves to `{}`", value_name(&other)),
                        *span,
                    )
                    .with_fix_it("apply `?` only to expressions that evaluate to `Result.Ok(...)` or `Result.Err(...)`")
                    .with_source_path(source_path.to_path_buf()),
                ])
                .into()),
            }
        }
        Expr::Unary { op, value, span } => {
            let value =
                eval_expr(value, scopes, functions, methods, structs, enums, output, source_path)?;
            Ok(eval_unary(*op, value, *span, source_path)?)
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
            Ok(eval_binary(lhs, *op, rhs, *span, source_path)?)
        }
    }
}

fn eval_unary(op: UnaryOp, value: Value, span: Span, source_path: &Path) -> EvalResult<Value> {
    match (op, value) {
        (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
        (UnaryOp::Neg, Value::Int(value)) => Ok(Value::Int(-value)),
        (UnaryOp::Not, other) => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3026",
                "`not` requires a `bool` operand",
                format!("this operand resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("apply `not` only to boolean expressions")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (UnaryOp::Neg, other) => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3065",
                "unary `-` requires an `int` operand",
                format!("this operand resolves to `{}`", value_name(&other)),
                span,
            )
            .with_fix_it("apply unary `-` only to integer expressions")
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
) -> EvalResult<Value> {
    let Value::Bool(lhs) = lhs else {
        return eval_diagnostics(Diagnostics(vec![
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
                other => eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
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
) -> EvalResult<Value> {
    if decl.fields.len() != args.len() {
        return eval_diagnostics(Diagnostics(vec![
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
    payloads: &[Value],
    span: Span,
    source_path: &Path,
) -> EvalResult<Value> {
    let Some(signature) = decl
        .variants
        .iter()
        .find(|candidate| candidate.name == variant)
    else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3028",
                format!("unknown variant `{variant}` on `{}`", decl.name),
                "enum variant references must use a variant declared on the enum",
                span,
            )
            .with_fix_it("use one of the variants declared on the enum")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if signature.fields.len() != payloads.len() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3069",
                format!(
                    "wrong number of payload values for `{}.{variant}`",
                    decl.name
                ),
                format!(
                    "expected {} payload value(s), got {}",
                    signature.fields.len(),
                    payloads.len()
                ),
                span,
            )
            .with_fix_it(format!(
                "construct it as `{}.{}`{}",
                decl.name,
                variant,
                if signature.fields.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        "({})",
                        signature
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ))
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::Enum(EnumValue {
        name: decl.name.clone(),
        variant: variant.to_string(),
        payloads: signature
            .fields
            .iter()
            .map(|field| field.name.clone())
            .zip(payloads.iter().cloned())
            .collect(),
    }))
}

fn eval_result_variant(
    variant: &str,
    payloads: &[Value],
    span: Span,
    source_path: &Path,
) -> EvalResult<Value> {
    let payload_name = match variant {
        "Ok" => "value",
        "Err" => "error",
        _ => {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3072",
                    format!("unknown result variant `Result.{variant}`"),
                    "the builtin result type exposes only `Result.Ok(value)` and `Result.Err(error)`",
                    span,
                )
                .with_fix_it("use `Result.Ok(value)` or `Result.Err(error)`")
                .with_source_path(source_path.to_path_buf()),
            ]));
        }
    };

    if payloads.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3073",
                format!("wrong number of payload values for `Result.{variant}`"),
                format!("expected 1 payload value, got {}", payloads.len()),
                span,
            )
            .with_fix_it(format!(
                "construct it as `Result.{variant}({payload_name})`"
            ))
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::Enum(EnumValue {
        name: "Result".to_string(),
        variant: variant.to_string(),
        payloads: vec![(payload_name.to_string(), payloads[0].clone())],
    }))
}

fn match_variant_field_names(
    expected_enum: &str,
    variant: &str,
    enums: &EnumTable,
    span: Span,
    source_path: &Path,
) -> EvalResult<Vec<String>> {
    if expected_enum == "Result" {
        return match variant {
            "Ok" => Ok(vec!["value".to_string()]),
            "Err" => Ok(vec!["error".to_string()]),
            _ => eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3072",
                    format!("unknown result variant `Result.{variant}`"),
                    "the builtin result type exposes only `Result.Ok(value)` and `Result.Err(error)`",
                    span,
                )
                .with_fix_it("use `Result.Ok(value)` or `Result.Err(error)`")
                .with_source_path(source_path.to_path_buf()),
            ])),
        };
    }

    let decl = enums.get(expected_enum).ok_or_else(|| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3031",
                "match arm uses an unknown enum",
                format!("`{expected_enum}` is not a declared enum in this module graph"),
                span,
            )
            .with_fix_it("use a declared enum name in the arm pattern")
            .with_source_path(source_path.to_path_buf()),
        ])
    })?;
    let Some(variant_decl) = decl
        .variants
        .iter()
        .find(|candidate| candidate.name == variant)
    else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3028",
                format!("unknown variant `{variant}` on `{expected_enum}`"),
                "enum match patterns must use a variant declared on the enum",
                span,
            )
            .with_fix_it("use one of the variants declared on the enum")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(variant_decl
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect())
}

fn resolve_match_pattern(
    pattern: &MatchPattern,
    expected_enum: &str,
    enums: &EnumTable,
    seen_variants: &mut HashSet<String>,
    source_path: &Path,
) -> EvalResult<ResolvedMatchPattern> {
    let MatchPattern::EnumVariant {
        enum_name,
        variant: field,
        bindings,
        span,
    } = pattern;

    if enum_name != expected_enum {
        return eval_diagnostics(Diagnostics(vec![
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

    if !seen_variants.insert(field.clone()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3030",
                format!("duplicate match arm for `{expected_enum}.{field}`"),
                "each enum variant can appear only once in a match over the same enum",
                *span,
            )
            .with_fix_it("remove the duplicate arm or replace it with another enum variant")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let variant_fields = match_variant_field_names(enum_name, field, enums, *span, source_path)?;

    if variant_fields.len() != bindings.len() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3071",
                format!(
                    "match arm for `{enum_name}.{field}` destructures the wrong number of payload values"
                ),
                format!(
                    "expected {} binding(s), got {}",
                    variant_fields.len(),
                    bindings.len()
                ),
                *span,
            )
            .with_fix_it(format!(
                "rewrite the arm as `{}.{}`{}",
                enum_name,
                field,
                if variant_fields.is_empty() {
                    "".to_string()
                } else {
                    format!("({})", variant_fields.join(", "))
                }
            ))
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(ResolvedMatchPattern {
        enum_name: enum_name.clone(),
        variant: field.clone(),
        bindings: bindings.clone(),
    })
}

fn eval_field_access(
    target: Value,
    field: &str,
    span: Span,
    source_path: &Path,
) -> EvalResult<Value> {
    match target {
        Value::Struct(value) => Ok(value.field(field).cloned().ok_or_else(|| {
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
        })?),
        other => eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
    match (lhs, op, rhs) {
        (Value::Int(lhs), BinaryOp::Add, Value::Int(rhs)) => Ok(Value::Int(lhs + rhs)),
        (Value::Int(lhs), BinaryOp::Sub, Value::Int(rhs)) => Ok(Value::Int(lhs - rhs)),
        (Value::Int(lhs), BinaryOp::Mul, Value::Int(rhs)) => Ok(Value::Int(lhs * rhs)),
        (Value::Int(_), BinaryOp::Div, Value::Int(0))
        | (Value::Int(_), BinaryOp::Mod, Value::Int(0)) => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3068",
                format!("`{}` by zero is not allowed", binary_op_name(op)),
                "the bootstrap numeric runtime rejects division and modulo by zero",
                span,
            )
            .with_fix_it("guard the divisor or ensure it is non-zero before evaluating this expression")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (Value::Int(lhs), BinaryOp::Div, Value::Int(rhs)) => Ok(Value::Int(lhs / rhs)),
        (Value::Int(lhs), BinaryOp::Mod, Value::Int(rhs)) => Ok(Value::Int(lhs % rhs)),
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
        _ => eval_diagnostics(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "the current evaluator supports int arithmetic, comparisons, and equality for strings, bools, lists, dicts, structs, and enums",
            span,
        )
        .with_source_path(source_path.to_path_buf())])),
    }
}

fn binary_op_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
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
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
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
        other => eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
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
        other => eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
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
        (Value::String(_), needle) => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string needle for string haystacks",
                format!("this needle resolves to `{}`", value_name(&needle)),
                args[1].span(),
            )
            .with_fix_it("pass a string as the second argument to `contains`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (Value::Dict(_), needle) => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string key for dict haystacks",
                format!("this needle resolves to `{}`", value_name(&needle)),
                args[1].span(),
            )
            .with_fix_it("pass a string key as the second argument to `contains`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        (other, _) => eval_diagnostics(Diagnostics(vec![
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

fn eval_trim_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `trim`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `trim(text)`")
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

    let Value::String(value) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3057",
                "`trim` requires a string value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a string value like `trim(\"  gof  \")`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(Value::String(value.trim().to_string()))
}

fn eval_split_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `split`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `split(text, separator)`")
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
    let separator = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::String(value) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3055",
                "`split` requires a string value as its first argument",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a string value as the first argument to `split`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::String(separator) = separator else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3055",
                "`split` requires a string separator",
                format!("this separator resolves to `{}`", value_name(&separator)),
                args[1].span(),
            )
            .with_fix_it("pass a string separator like `\",\"` to `split`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if separator.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3055",
                "`split` requires a non-empty separator",
                "an empty separator would make the bootstrap string contract ambiguous",
                args[1].span(),
            )
            .with_fix_it("pass a visible separator such as `\",\"` or `\"-\"`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::List(
        value
            .split(&separator)
            .map(|part| Value::String(part.to_string()))
            .collect(),
    ))
}

fn eval_join_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `join`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `join(parts, separator)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let parts = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let separator = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::List(parts) = parts else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3056",
                "`join` requires a list of strings as its first argument",
                format!("this argument resolves to `{}`", value_name(&parts)),
                args[0].span(),
            )
            .with_fix_it("pass a `list[string]` value to `join`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::String(separator) = separator else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3056",
                "`join` requires a string separator",
                format!("this separator resolves to `{}`", value_name(&separator)),
                args[1].span(),
            )
            .with_fix_it("pass a string separator as the second argument to `join`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let mut string_parts = Vec::with_capacity(parts.len());
    for part in parts {
        let Value::String(part) = part else {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3056",
                    "`join` requires a list of strings as its first argument",
                    format!("this list contains `{}`", value_name(&part)),
                    args[0].span(),
                )
                .with_fix_it("ensure every element passed to `join` is a string")
                .with_source_path(source_path.to_path_buf()),
            ]));
        };
        string_parts.push(part);
    }

    Ok(Value::String(string_parts.join(&separator)))
}

fn eval_starts_with_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    eval_string_pair_builtin(
        args,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        span,
        "starts_with",
        "GOF3058",
        |value, pattern| Value::Bool(value.starts_with(pattern)),
    )
}

fn eval_ends_with_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    eval_string_pair_builtin(
        args,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        span,
        "ends_with",
        "GOF3059",
        |value, pattern| Value::Bool(value.ends_with(pattern)),
    )
}

fn eval_parse_int_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `parse_int`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `parse_int(text)`")
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

    let Value::String(value_text) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3060",
                "`parse_int` requires a string value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a string like `\"42\"` or `trim(text)` to `parse_int`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(value_text.parse::<i64>().map(Value::Int).map_err(|error| {
        Diagnostics(vec![
            Diagnostic::error(
                "GOF3061",
                format!("failed to parse int from `{value_text}`"),
                error.to_string(),
                args[0].span(),
            )
            .with_fix_it("pass a base-10 integer string such as `\"42\"` or `\"-7\"`")
            .with_source_path(source_path.to_path_buf()),
        ])
    })?)
}

fn eval_to_string_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `to_string`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `to_string(value)`")
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
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3062",
                "`to_string` requires a printable value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it(
                "pass an int, string, bool, list, dict, struct, or enum value to `to_string`",
            )
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(Value::String(rendered))
}

fn eval_range_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if !(1..=3).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `range`",
                format!("expected 1, 2, or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `range(stop)`, `range(start, stop)`, or `range(start, stop, step)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let values = args
        .iter()
        .map(|arg| {
            eval_expr(
                arg,
                scopes,
                functions,
                methods,
                structs,
                enums,
                output,
                source_path,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let ints = values
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::Int(number) => Ok(*number),
            other => eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3063",
                    format!("`range` argument {} must resolve to `int`", index + 1),
                    format!("this argument resolves to `{}`", value_name(other)),
                    args[index].span(),
                )
                .with_fix_it("pass integer values to `range`")
                .with_source_path(source_path.to_path_buf()),
            ])),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let (mut current, stop, step) = match ints.as_slice() {
        [stop] => (0, *stop, 1),
        [start, stop] => (*start, *stop, 1),
        [start, stop, step] => (*start, *stop, *step),
        _ => unreachable!("range arity validated earlier"),
    };

    if step == 0 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3064",
                "`range` step cannot be zero",
                "a zero step would never make forward progress",
                args[args.len() - 1].span(),
            )
            .with_fix_it("pass a non-zero integer step to `range`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let mut items = Vec::new();
    if step > 0 {
        while current < stop {
            items.push(Value::Int(current));
            current += step;
        }
    } else {
        while current > stop {
            items.push(Value::Int(current));
            current += step;
        }
    }

    Ok(Value::List(items))
}

fn eval_string_pair_builtin<F>(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
    callee: &'static str,
    code: &'static str,
    apply: F,
) -> EvalResult<Value>
where
    F: FnOnce(&str, &str) -> Value,
{
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{callee}`"),
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{callee}(text, pattern)`"))
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
    let pattern = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::String(value_text) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                code,
                format!("`{callee}` requires a string value as its first argument"),
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it(format!(
                "pass a string value as the first argument to `{callee}`"
            ))
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::String(pattern_text) = pattern else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                code,
                format!("`{callee}` requires a string value as its second argument"),
                format!("this argument resolves to `{}`", value_name(&pattern)),
                args[1].span(),
            )
            .with_fix_it(format!(
                "pass a string value as the second argument to `{callee}`"
            ))
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(apply(&value_text, &pattern_text))
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
) -> EvalResult<Value> {
    if !(1..=2).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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
            return eval_diagnostics(Diagnostics(vec![
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
        eval_diagnostics(Diagnostics(vec![
            Diagnostic::error("GOF3042", "assertion failed", detail, args[0].span())
                .with_fix_it("adjust the asserted condition or the preceding logic")
                .with_source_path(source_path.to_path_buf()),
        ]))
    }
}

fn eval_argv_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `argv`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `argv()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::List(
        host_program_args().into_iter().map(Value::String).collect(),
    ))
}

fn eval_env_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `env`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `env(name)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let name = eval_string_argument(
        "env",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    Ok(match std::env::var(&name) {
        Ok(value) => result_ok(Value::String(value)),
        Err(_) => result_err(runtime_env_missing_error(&name)),
    })
}

fn eval_cwd_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `cwd`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cwd()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(match std::env::current_dir() {
        Ok(path) => result_ok(Value::String(path.to_string_lossy().into_owned())),
        Err(error) => result_err(runtime_io_error(error.to_string())),
    })
}

fn eval_exists_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `exists`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `exists(path)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_text = eval_string_argument(
        "exists",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;
    Ok(Value::Bool(Path::new(&path_text).exists()))
}

fn eval_read_dir_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `read_dir`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_dir(path)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_text = eval_string_argument(
        "read_dir",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    let entries = fs::read_dir(&path_text)
        .map_err(|error| runtime_io_error(error.to_string()))
        .and_then(|read_dir| {
            let mut values = read_dir
                .map(|entry| {
                    entry
                        .map(|value| {
                            Value::String(value.file_name().to_string_lossy().into_owned())
                        })
                        .map_err(|error| runtime_io_error(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            values.sort_by_key(|value| value.cli_text().unwrap_or_default());
            Ok(values)
        });

    Ok(match entries {
        Ok(values) => result_ok(Value::List(values)),
        Err(error) => result_err(error),
    })
}

fn eval_mkdir_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `mkdir`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `mkdir(path)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_text = eval_string_argument(
        "mkdir",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    Ok(match fs::create_dir_all(&path_text) {
        Ok(()) => result_ok(Value::Unit),
        Err(error) => result_err(runtime_io_error(error.to_string())),
    })
}

fn eval_remove_file_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `remove_file`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `remove_file(path)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let path_text = eval_string_argument(
        "remove_file",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    Ok(match fs::remove_file(&path_text) {
        Ok(()) => result_ok(Value::Unit),
        Err(error) => result_err(runtime_io_error(error.to_string())),
    })
}

fn eval_path_join_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `path_join`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `path_join(left, right)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let left = eval_string_argument(
        "path_join",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;
    let right = eval_string_argument(
        "path_join",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    Ok(Value::String(
        PathBuf::from(left)
            .join(right)
            .to_string_lossy()
            .into_owned(),
    ))
}

fn eval_path_dir_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    let path_text = eval_unary_path_like_builtin(
        "path_dir",
        args,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        span,
    )?;
    let value = Path::new(&path_text)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Value::String(value))
}

fn eval_path_base_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    let path_text = eval_unary_path_like_builtin(
        "path_base",
        args,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        span,
    )?;
    let value = Path::new(&path_text)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Value::String(value))
}

fn eval_path_ext_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    let path_text = eval_unary_path_like_builtin(
        "path_ext",
        args,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        span,
    )?;
    let value = Path::new(&path_text)
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Value::String(value))
}

fn eval_unary_path_like_builtin(
    name: &str,
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<String> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{name}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{name}(path)`"))
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    eval_string_argument(
        name,
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )
}

fn eval_string_argument(
    builtin_name: &str,
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    diagnostic_code: &'static str,
) -> EvalResult<String> {
    let value = eval_expr(
        expr,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::String(text) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                diagnostic_code,
                format!("`{builtin_name}` requires a string argument"),
                format!("this argument resolves to `{}`", value_name(&value)),
                expr.span(),
            )
            .with_fix_it("pass a string value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    Ok(text)
}

fn eval_int_argument(
    builtin_name: &str,
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    diagnostic_code: &'static str,
) -> EvalResult<i64> {
    let value = eval_expr(
        expr,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::Int(number) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                diagnostic_code,
                format!("`{builtin_name}` requires an integer argument"),
                format!("this argument resolves to `{}`", value_name(&value)),
                expr.span(),
            )
            .with_fix_it("pass an integer value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    Ok(number)
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
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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

    Ok(match fs::read_to_string(&path_text) {
        Ok(contents) => result_ok(Value::String(contents)),
        Err(error) => result_err(runtime_io_error(error.to_string())),
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
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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

    Ok(match fs::write(&path_text, contents) {
        Ok(()) => result_ok(Value::Unit),
        Err(error) => result_err(runtime_io_error(error.to_string())),
    })
}

fn eval_dict_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
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
) -> EvalResult<Value> {
    if args.len() != 3 {
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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
        return eval_diagnostics(Diagnostics(vec![
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

fn eval_keys_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `keys`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `keys(dict_value)`")
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

    let Value::Dict(dict_value) = dict_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3051",
                "`keys` requires a dict value",
                format!("this argument resolves to `{}`", value_name(&dict_value)),
                args[0].span(),
            )
            .with_fix_it("pass a dict value like `keys(metrics)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(Value::List(dict_value.keys_list()))
}

fn eval_values_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `values`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `values(dict_value)`")
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

    let Value::Dict(dict_value) = dict_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3052",
                "`values` requires a dict value",
                format!("this argument resolves to `{}`", value_name(&dict_value)),
                args[0].span(),
            )
            .with_fix_it("pass a dict value like `values(metrics)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(Value::List(dict_value.values_list()))
}

fn eval_channel_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
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

fn eval_close_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `close`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `close(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let channel = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::Channel(channel) = channel else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3081",
                "`close` requires a channel value",
                format!("this argument resolves to `{}`", value_name(&channel)),
                args[0].span(),
            )
            .with_fix_it("pass a channel value to `close`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    channel.close();
    Ok(Value::Unit)
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
) -> EvalResult<Value> {
    if !(2..=3).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `send`",
                format!("expected 2 or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `send(channel_value, item)` or `send(channel_value, item, token)`")
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
    let cancel_token = if args.len() == 3 {
        Some(eval_cancel_token_argument(
            &args[2],
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

    let Value::Channel(channel_value) = channel_value else {
        return eval_diagnostics(Diagnostics(vec![
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
    Ok(
        match channel_value.send(sent_value, cancel_token.as_ref()) {
            ChannelReceiveState::Value(_) => result_ok(Value::Unit),
            ChannelReceiveState::Closed => result_err(runtime_channel_closed_error()),
            ChannelReceiveState::Cancelled => result_err(runtime_cancelled_error()),
        },
    )
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
) -> EvalResult<Value> {
    if !(1..=2).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `recv`",
                format!("expected 1 or 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `recv(channel_value)` or `recv(channel_value, token)`")
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
    let cancel_token = if args.len() == 2 {
        Some(eval_cancel_token_argument(
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
    let Value::Channel(channel_value) = channel_value else {
        return eval_diagnostics(Diagnostics(vec![
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
    Ok(match channel_value.recv(cancel_token.as_ref()) {
        ChannelReceiveState::Value(value) => result_ok(value),
        ChannelReceiveState::Closed => result_err(runtime_channel_closed_error()),
        ChannelReceiveState::Cancelled => result_err(runtime_cancelled_error()),
    })
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
) -> EvalResult<Option<Value>> {
    match operation {
        Expr::Call { callee, args, span } if callee == "recv" => {
            if !(1..=2).contains(&args.len()) {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` arms currently require `recv(channel)` operations",
                        "each select arm must call `recv` with one channel argument and an optional cancellation token",
                        *span,
                    )
                    .with_fix_it("rewrite the arm as `recv(channel):`, `recv(channel, token):`, `value = recv(channel):`, or `value = recv(channel, token):`")
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
            let cancel_token = if args.len() == 2 {
                Some(eval_cancel_token_argument(
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
            let Value::Channel(channel_value) = channel_value else {
                return eval_diagnostics(Diagnostics(vec![
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

            Ok(channel_value
                .try_recv(cancel_token.as_ref())
                .map(|state| match state {
                    ChannelReceiveState::Value(value) => result_ok(value),
                    ChannelReceiveState::Closed => result_err(runtime_channel_closed_error()),
                    ChannelReceiveState::Cancelled => result_err(runtime_cancelled_error()),
                }))
        }
        Expr::Call { callee, .. } => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(channel)` operations",
                format!("this arm uses `{callee}(...)` instead"),
                operation.span(),
            )
            .with_fix_it("replace the arm operation with `recv(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        _ => eval_diagnostics(Diagnostics(vec![
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

fn eval_cancel_token_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `cancel_token`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cancel_token()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(Value::CancelToken(CancelTokenValue::new()))
}

fn eval_cancel_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `cancel`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cancel(token)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let token = eval_cancel_token_argument(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    token.cancel();
    Ok(Value::Unit)
}

fn eval_is_cancelled_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `is_cancelled`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `is_cancelled(token)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let token = eval_cancel_token_argument(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    Ok(Value::Bool(token.is_cancelled()))
}

fn eval_cancel_token_argument(
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
) -> EvalResult<CancelTokenValue> {
    let value = eval_expr(
        expr,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::CancelToken(token) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3082",
                "this builtin requires a cancellation token",
                format!("this argument resolves to `{}`", value_name(&value)),
                expr.span(),
            )
            .with_fix_it("pass a value created by `cancel_token()`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    Ok(token)
}

fn eval_json_parse_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_parse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_parse(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "json_parse",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;
    Ok(match serde_json::from_str::<SerdeJsonValue>(&text) {
        Ok(json) => match json_from_serde(json) {
            Ok(value) => result_ok(Value::Json(value)),
            Err(message) => result_err(runtime_json_error(message)),
        },
        Err(error) => result_err(runtime_json_error(error.to_string())),
    })
}

fn eval_json_stringify_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_stringify`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_stringify(value)`")
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
    let Value::Json(value) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_stringify` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a value returned from `json_parse(...)` or another json helper")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(result_ok(Value::String(value.cli_text())))
}

fn eval_json_get_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_get`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_get(value, \"key\")`")
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
    let key = eval_string_argument(
        "json_get",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    let Value::Json(json) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_get` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match json {
        JsonValue::Object(entries) => match entries.get(&key) {
            Some(value) => result_ok(Value::Json(value.clone())),
            None => result_err(runtime_json_error(format!("missing key `{key}`"))),
        },
        _ => result_err(runtime_json_error("json_get expects a JSON object")),
    })
}

fn eval_json_index_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 2 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_index`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_index(value, index)`")
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
    let index = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::Json(json) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_index` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::Int(index) = index else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3084",
                "`json_index` requires an integer index",
                format!("this index resolves to `{}`", value_name(&index)),
                args[1].span(),
            )
            .with_fix_it("pass an integer index like `0`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match json {
        JsonValue::Array(items) => {
            if index < 0 {
                result_err(runtime_json_error(format!(
                    "json_index out of bounds for index {index}"
                )))
            } else {
                items
                    .get(index as usize)
                    .cloned()
                    .map(Value::Json)
                    .map(result_ok)
                    .unwrap_or_else(|| {
                        result_err(runtime_json_error(format!(
                            "json_index out of bounds for index {index}"
                        )))
                    })
            }
        }
        _ => result_err(runtime_json_error("json_index expects a JSON array")),
    })
}

fn eval_json_len_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_len`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_len(value)`")
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
    let Value::Json(json) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_len` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match json {
        JsonValue::Array(items) => result_ok(Value::Int(items.len() as i64)),
        JsonValue::Object(entries) => result_ok(Value::Int(entries.len() as i64)),
        _ => result_err(runtime_json_error("json_len expects an array or object")),
    })
}

fn eval_json_string_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_string`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_string(value)`")
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
    let Value::Json(json) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_string` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match json {
        JsonValue::String(value) => result_ok(Value::String(value)),
        _ => result_err(runtime_json_error("json_string expects a JSON string")),
    })
}

fn eval_json_int_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_int`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_int(value)`")
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
    let Value::Json(json) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3083",
                "`json_int` requires a `json` value",
                format!("this argument resolves to `{}`", value_name(&value)),
                args[0].span(),
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match json {
        JsonValue::Int(value) => result_ok(Value::Int(value)),
        _ => result_err(runtime_json_error("json_int expects a JSON integer")),
    })
}

fn eval_http_get_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `http_get`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `http_get(url)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let url = eval_string_argument(
        "http_get",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;

    Ok(eval_http_response(
        ureq::get(&url).timeout(Duration::from_secs(35)).call(),
    ))
}

fn eval_http_post_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if !(2..=3).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `http_post`",
                format!("expected 2 or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `http_post(url, body)` or `http_post(url, body, content_type)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let url = eval_string_argument(
        "http_post",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;
    let body = eval_string_argument(
        "http_post",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3076",
    )?;
    let content_type = if args.len() == 3 {
        eval_string_argument(
            "http_post",
            &args[2],
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
            "GOF3076",
        )?
    } else {
        "text/plain; charset=utf-8".to_string()
    };

    Ok(eval_http_response(
        ureq::post(&url)
            .set("Content-Type", &content_type)
            .timeout(Duration::from_secs(35))
            .send_string(&body),
    ))
}

fn eval_sleep_builtin(
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `sleep`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `sleep(milliseconds)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let millis = eval_int_argument(
        "sleep",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3085",
    )?;

    if millis < 0 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3086",
                "`sleep` requires a non-negative duration",
                format!("this duration resolves to `{millis}`"),
                args[0].span(),
            )
            .with_fix_it("pass `0` or another non-negative millisecond count")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    std::thread::sleep(Duration::from_millis(millis as u64));
    Ok(Value::Unit)
}

fn eval_http_response(response: Result<ureq::Response, ureq::Error>) -> Value {
    match response {
        Ok(response) => {
            let status = i64::from(response.status());
            match response.into_string() {
                Ok(body) if (200..300).contains(&status) => result_ok(Value::String(body)),
                Ok(body) => result_err(runtime_http_status_error(status, body)),
                Err(error) => result_err(runtime_http_request_error(error.to_string())),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_else(|_| String::new());
            result_err(runtime_http_status_error(i64::from(code), body))
        }
        Err(ureq::Error::Transport(error)) => {
            result_err(runtime_http_request_error(error.to_string()))
        }
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

fn iter_values_from_iterable(
    iterable: &Value,
    span: Span,
    source_path: &Path,
) -> Result<Vec<Value>, Diagnostics> {
    match iterable {
        Value::List(values) => Ok(values.clone()),
        Value::String(value) => Ok(value
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()),
        Value::Dict(values) => Ok(values.entries.keys().cloned().map(Value::String).collect()),
        other => Err(Diagnostics(vec![
            Diagnostic::error(
                "GOF3048",
                "`for` currently requires an iterable value",
                format!(
                    "this loop target resolves to `{}` instead of `list`, `string`, or `dict`",
                    value_name(other)
                ),
                span,
            )
            .with_fix_it("iterate over a list, string, or dict value")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn value_name(value: &Value) -> &'static str {
    match value {
        Value::Int(_) => "int",
        Value::String(_) => "string",
        Value::Bool(_) => "bool",
        Value::Json(_) => "json",
        Value::Struct(_) => "struct",
        Value::Enum(_) => "enum",
        Value::List(_) => "list",
        Value::Dict(_) => "dict",
        Value::Channel(_) => "channel",
        Value::CancelToken(_) => "cancel_token",
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
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
    fn evaluates_dict_literals() {
        let value = run_source(
            "fn main() -> int:\n    values = {\"ok\": 2, \"warn\": 3}\n    return values[\"ok\"] + len(values)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(4));
    }

    #[test]
    fn evaluates_for_in_over_core_iterables() {
        let value = run_source(
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    for ch in \"go\":\n        total = total + len(ch)\n    mut store: dict = dict()\n    store = insert(store, \"alpha\", 1)\n    for key in store:\n        total = total + len(key)\n    return total\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(13));
    }

    #[test]
    fn evaluates_break_and_continue_inside_loops() {
        let value = run_source(
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3, 4]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(4));
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
    fn evaluates_string_helpers() {
        let value = run_source(
            "fn main() -> int:\n    line = trim(\"  gof,lang  \")\n    parts = split(line, \",\")\n    merged = join(parts, \"-\")\n    if starts_with(merged, \"gof\") and ends_with(merged, \"lang\"):\n        return len(merged) + len(parts)\n    return 0\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(10));
    }

    #[test]
    fn evaluates_conversion_builtins() {
        let value = run_source(
            "fn main() -> int:\n    parsed = parse_int(trim(\" 41 \"))\n    rendered = \"gof-\" + to_string(parsed + 1)\n    assert(rendered == \"gof-42\", \"expected converted text\")\n    return parsed + len(rendered)\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(47));
    }

    #[test]
    fn evaluates_sleep_builtin() {
        let value = run_source("fn main() -> int:\n    sleep(0)\n    return 1\n")
            .expect("program should run");
        assert_eq!(value, Value::Int(1));
    }

    #[test]
    fn evaluates_http_post_builtin() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener addr should exist");
        let observed_requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_requests_thread = observed_requests.clone();

        let server = std::thread::spawn(move || {
            fn expected_request_len(bytes: &[u8]) -> Option<usize> {
                let header_end = bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|index| index + 4)?;
                let headers = std::str::from_utf8(&bytes[..header_end]).ok()?;
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .or_else(|| line.strip_prefix("content-length: "))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                Some(header_end + content_length)
            }

            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 4096];
            let mut request_bytes = Vec::new();
            loop {
                let size = stream
                    .read(&mut buffer)
                    .expect("request should be readable");
                if size == 0 {
                    break;
                }
                request_bytes.extend_from_slice(&buffer[..size]);
                if let Some(total_len) = expected_request_len(&request_bytes) {
                    if request_bytes.len() >= total_len {
                        request_bytes.truncate(total_len);
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request_bytes).to_string();
            observed_requests_thread
                .lock()
                .expect("requests mutex should not be poisoned")
                .push(request);

            let body = "done";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        });

        let value = run_source(&format!(
            "fn main() -> Result[int, RuntimeError]:\n    body = http_post(\"http://{address}/submit\", \"payload\", \"text/plain\")?\n    return Result.Ok(len(body))\n"
        ))
        .expect("program should run");

        server.join().expect("server thread should exit");
        let requests = observed_requests
            .lock()
            .expect("requests mutex should not be poisoned")
            .clone();
        assert!(
            requests
                .iter()
                .any(|request| request.contains("POST /submit HTTP/1.1")),
            "expected POST request, got {requests:?}"
        );
        assert!(
            requests.iter().any(|request| request.contains("payload")),
            "expected payload body, got {requests:?}"
        );
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 4)"));
    }

    #[test]
    fn evaluates_unary_minus_division_and_modulo() {
        let value = run_source("fn main() -> int:\n    base = -6 / 3\n    return base % 4\n")
            .expect("program should run");
        assert_eq!(value, Value::Int(-2));
    }

    #[test]
    fn evaluates_range_builtin() {
        let value = run_source(
            "fn main() -> int:\n    mut total = 0\n    for value in range(1, 7, 2):\n        total = total + value\n    for value in range(3):\n        total = total + value\n    return total\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(12));
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
            "fn main() -> Result[int, RuntimeError]:\n    path = \"{input}\"\n    out = \"{output}\"\n    mut data: dict = dict()\n    contents = read_file(path)?\n    data = insert(data, \"size\", len(contents))\n    assert(contains(data, \"size\"), \"missing size\")\n    write_file(out, contents)?\n    return Result.Ok(data[\"size\"])\n"
        ))
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 3)"));
        assert_eq!(
            std::fs::read_to_string(&output_path).expect("output file should exist"),
            "gof"
        );
    }

    #[test]
    fn evaluates_dict_view_builtins() {
        let value = run_source(
            "fn main() -> int:\n    metrics: dict = {\"critical\": 5, \"ok\": 7, \"warn\": 2}\n    names = keys(metrics)\n    counts = values(metrics)\n    assert(names[0] == \"critical\", \"expected deterministic order\")\n    mut total = 0\n    for name in names:\n        total = total + len(name)\n    for count in counts:\n        total = total + count\n    return total\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(28));
    }

    #[test]
    fn evaluates_channels_and_select() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    left: channel = channel()\n    right: channel = channel()\n    send(right, 8)?\n    select:\n        received = recv(left):\n            match received:\n                Result.Ok(value):\n                    return Result.Ok(value)\n                Result.Err(error):\n                    return Result.Err(error)\n        received = recv(right):\n            match received:\n                Result.Ok(value):\n                    return Result.Ok(value + 1)\n                Result.Err(error):\n                    return Result.Err(error)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 9)"));
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
    fn evaluates_payload_enums_and_destructuring() {
        let value = run_source(
            "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn score(state: JobState) -> int:\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n\nfn main() -> int:\n    return score(JobState.Running(42))\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn evaluates_result_propagation_and_match() {
        let value = run_source(
            "enum MathError:\n    TooSmall\n    NotEven(value: int)\n\nfn halve(value: int) -> Result[int, MathError]:\n    if value < 2:\n        return Result.Err(MathError.TooSmall)\n    if value % 2 != 0:\n        return Result.Err(MathError.NotEven(value))\n    return Result.Ok(value / 2)\n\nfn compute() -> Result[int, MathError]:\n    half = halve(84)?\n    return Result.Ok(half)\n\nfn main() -> int:\n    outcome = compute()\n    match outcome:\n        Result.Ok(value):\n            return value\n        Result.Err(error):\n            match error:\n                MathError.TooSmall:\n                    return 0\n                MathError.NotEven(value):\n                    return value\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(42));
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

    #[test]
    fn rejects_payload_enum_constructor_arity_mismatches() {
        let diagnostics = run_source(
            "enum JobState:\n    Running(pid: int)\n\nfn main() -> JobState:\n    return JobState.Running()\n",
        )
        .expect_err("payload enum arity should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3069"]);
    }

    #[test]
    fn rejects_invalid_for_iterables() {
        let diagnostics =
            run_source("fn main() -> int:\n    for value in 42:\n        return value\n")
                .expect_err("invalid loop target should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3048"]);
    }

    #[test]
    fn rejects_break_outside_loops() {
        let diagnostics = run_source("fn main() -> int:\n    break\n    return 0\n")
            .expect_err("break outside loop should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3053"]);
    }

    #[test]
    fn rejects_continue_outside_loops() {
        let diagnostics = run_source("fn main() -> int:\n    continue\n    return 0\n")
            .expect_err("continue outside loop should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3054"]);
    }

    #[test]
    fn rejects_invalid_keys_operand() {
        let diagnostics = run_source("fn main() -> list:\n    return keys(1)\n")
            .expect_err("keys operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3051"]);
    }

    #[test]
    fn rejects_invalid_values_operand() {
        let diagnostics = run_source("fn main() -> list:\n    return values(false)\n")
            .expect_err("values operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3052"]);
    }

    #[test]
    fn rejects_invalid_split_operand() {
        let diagnostics = run_source("fn main() -> list:\n    return split(1, \",\")\n")
            .expect_err("split operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3055"]);
    }

    #[test]
    fn rejects_invalid_join_operand() {
        let diagnostics = run_source("fn main() -> string:\n    return join([1, 2], \",\")\n")
            .expect_err("join operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3056"]);
    }

    #[test]
    fn rejects_invalid_trim_operand() {
        let diagnostics = run_source("fn main() -> string:\n    return trim(1)\n")
            .expect_err("trim operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3057"]);
    }

    #[test]
    fn rejects_invalid_starts_with_operand() {
        let diagnostics = run_source("fn main() -> bool:\n    return starts_with(\"gof\", 1)\n")
            .expect_err("starts_with operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3058"]);
    }

    #[test]
    fn rejects_invalid_ends_with_operand() {
        let diagnostics = run_source("fn main() -> bool:\n    return ends_with(false, \"gof\")\n")
            .expect_err("ends_with operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3059"]);
    }

    #[test]
    fn rejects_invalid_parse_int_operand() {
        let diagnostics = run_source("fn main() -> int:\n    return parse_int(1)\n")
            .expect_err("parse_int operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3060"]);
    }

    #[test]
    fn rejects_invalid_sleep_operand() {
        let diagnostics = run_source("fn main() -> unit:\n    sleep(\"soon\")\n")
            .expect_err("sleep operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3085"]);
    }

    #[test]
    fn rejects_invalid_parse_int_text() {
        let diagnostics = run_source("fn main() -> int:\n    return parse_int(\"oops\")\n")
            .expect_err("parse_int text should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3061"]);
    }

    #[test]
    fn rejects_invalid_to_string_operand() {
        let diagnostics = run_source("fn main() -> string:\n    return to_string(channel())\n")
            .expect_err("to_string operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3062"]);
    }

    #[test]
    fn rejects_invalid_range_operand() {
        let diagnostics = run_source("fn main() -> list:\n    return range(\"bad\")\n")
            .expect_err("range operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3063"]);
    }

    #[test]
    fn rejects_zero_range_step() {
        let diagnostics = run_source("fn main() -> list:\n    return range(0, 5, 0)\n")
            .expect_err("range zero step should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3064"]);
    }

    #[test]
    fn rejects_division_by_zero() {
        let diagnostics = run_source("fn main() -> int:\n    return 6 / 0\n")
            .expect_err("division by zero should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3068"]);
    }

    #[test]
    fn rejects_invalid_dict_literal_keys() {
        let diagnostics = run_source("fn main() -> dict:\n    return {1: 2}\n")
            .expect_err("dict key should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3049"]);
    }
}
