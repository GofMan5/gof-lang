use crate::ast::{
    BinaryOp, EnumDecl, EnumVariant, Expr, Function, MatchPattern, Module, Param, SelectArm,
    SelectArmKind, Stmt, StructDecl, UnaryOp,
};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::source::Span;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use csv::{ReaderBuilder, WriterBuilder};
use serde_json::Value as SerdeJsonValue;
use serde_yaml::Value as YamlValue;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::{Debug, Formatter};
use std::fs;
use std::io::Read;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use toml::Value as TomlValue;

type FunctionTable = Arc<HashMap<String, Function>>;
type MethodTable = Arc<HashMap<(String, String), Function>>;
type StructTable = Arc<HashMap<String, StructDecl>>;
type EnumTable = Arc<HashMap<String, EnumDecl>>;

thread_local! {
    static NEXT_SELECT_ARM_START: Cell<usize> = const { Cell::new(0) };
}

fn reset_select_arm_rotation() {
    NEXT_SELECT_ARM_START.with(|counter| counter.set(0));
}

fn next_select_arm_start(arm_count: usize) -> usize {
    NEXT_SELECT_ARM_START.with(|counter| {
        let current = counter.get();
        counter.set(current.wrapping_add(1));
        current % arm_count
    })
}

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

fn json_from_toml(value: TomlValue) -> Result<JsonValue, String> {
    match value {
        TomlValue::String(value) => Ok(JsonValue::String(value)),
        TomlValue::Integer(value) => Ok(JsonValue::Int(value)),
        TomlValue::Boolean(value) => Ok(JsonValue::Bool(value)),
        TomlValue::Array(values) => values
            .into_iter()
            .map(json_from_toml)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        TomlValue::Table(entries) => entries
            .into_iter()
            .map(|(key, value)| json_from_toml(value).map(|json| (key, json)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(JsonValue::Object),
        TomlValue::Float(value) => Err(format!(
            "TOML float values are not supported in the bootstrap json bridge (`{value}`)"
        )),
        TomlValue::Datetime(value) => Err(format!(
            "TOML datetime values are not supported in the bootstrap json bridge (`{value}`)"
        )),
    }
}

fn json_from_yaml(value: YamlValue) -> Result<JsonValue, String> {
    match value {
        YamlValue::Null => Ok(JsonValue::Null),
        YamlValue::Bool(value) => Ok(JsonValue::Bool(value)),
        YamlValue::Number(number) => number.as_i64().map(JsonValue::Int).ok_or_else(|| {
            format!(
                "only integer YAML numbers are supported in the bootstrap json bridge (`{number}`)"
            )
        }),
        YamlValue::String(value) => Ok(JsonValue::String(value)),
        YamlValue::Sequence(values) => values
            .into_iter()
            .map(json_from_yaml)
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        YamlValue::Mapping(entries) => entries
            .into_iter()
            .map(|(key, value)| match key {
                YamlValue::String(key) => json_from_yaml(value).map(|json| (key, json)),
                other => Err(format!(
                    "YAML mapping keys must be strings in the bootstrap json bridge, got `{}`",
                    yaml_type_name(&other)
                )),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(JsonValue::Object),
        YamlValue::Tagged(tagged) => Err(format!(
            "YAML tagged values are not supported in the bootstrap json bridge (`{}`)",
            tagged.tag
        )),
    }
}

fn yaml_type_name(value: &YamlValue) -> &'static str {
    match value {
        YamlValue::Null => "null",
        YamlValue::Bool(_) => "bool",
        YamlValue::Number(_) => "number",
        YamlValue::String(_) => "string",
        YamlValue::Sequence(_) => "sequence",
        YamlValue::Mapping(_) => "mapping",
        YamlValue::Tagged(_) => "tagged",
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

    fn cancel_after(&self, millis: i64) {
        if millis == 0 {
            self.cancel();
            return;
        }

        let token = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(millis as u64));
            token.cancel();
        });
    }

    fn timeout_after(millis: i64) -> Self {
        let token = Self::new();
        token.cancel_after(millis);
        token
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
                    name: "Time".to_string(),
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
                    name: "Yaml".to_string(),
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
                    name: "Base64".to_string(),
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
                    name: "TaskFailed".to_string(),
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
                    name: "TaskPanicked".to_string(),
                    fields: vec![crate::ast::EnumVariantField {
                        name: "task".to_string(),
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
                    name: "ParseInt".to_string(),
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
                    name: "EmptySequence".to_string(),
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
                    name: "Slice".to_string(),
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
                    name: "Csv".to_string(),
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
                    name: "Toml".to_string(),
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
                    name: "Template".to_string(),
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

    fn try_send(
        &self,
        value: Value,
        token: Option<&CancelTokenValue>,
    ) -> Option<ChannelReceiveState> {
        self.0.try_send(value, token)
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

    fn await_result_value(&self, token: Option<&CancelTokenValue>) -> Value {
        self.0.await_result_value(token)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskBoundary {
    Direct,
    RuntimeResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskPanic {
    function_name: String,
    source_path: PathBuf,
    span: Span,
}

#[derive(Debug, Clone)]
enum TaskOutcome {
    Value(Value),
    Diagnostics(Diagnostics),
    Panic(TaskPanic),
}

#[derive(Debug)]
struct TaskHandle {
    boundary: TaskBoundary,
    result: Arc<(Mutex<Option<TaskOutcome>>, Condvar)>,
}

#[derive(Debug)]
struct ChannelState {
    queue: VecDeque<Value>,
    rendezvous_slot: Option<Value>,
    closed: bool,
    capacity: Option<usize>,
    waiting_receivers: usize,
}

#[derive(Debug)]
struct ChannelHandle {
    state: Mutex<ChannelState>,
    ready: Condvar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChannelReceiveState {
    Value(Value),
    Closed,
    Cancelled,
}

#[derive(Clone, Debug)]
struct OutputBuffer {
    stdout: Arc<Mutex<String>>,
    program_args: Arc<Vec<String>>,
    stdin: Arc<Mutex<StdinState>>,
}

#[derive(Debug)]
enum StdinState {
    HostPending,
    Provided(String),
    Loaded(Result<String, String>),
}

impl OutputBuffer {
    fn new(program_args: Vec<String>) -> Self {
        Self::new_with_optional_stdin(program_args, None)
    }

    fn new_with_stdin(program_args: Vec<String>, stdin: String) -> Self {
        Self::new_with_optional_stdin(program_args, Some(stdin))
    }

    fn new_with_optional_stdin(program_args: Vec<String>, stdin: Option<String>) -> Self {
        Self {
            stdout: Arc::new(Mutex::new(String::new())),
            program_args: Arc::new(program_args),
            stdin: Arc::new(Mutex::new(match stdin {
                Some(text) => StdinState::Provided(text),
                None => StdinState::HostPending,
            })),
        }
    }

    fn push_line(&self, line: &str) {
        let mut buffer = self
            .stdout
            .lock()
            .expect("output buffer mutex should not be poisoned");
        buffer.push_str(line);
        buffer.push('\n');
    }

    fn snapshot(&self) -> String {
        self.stdout
            .lock()
            .expect("output buffer mutex should not be poisoned")
            .clone()
    }

    fn program_args(&self) -> &[String] {
        self.program_args.as_ref().as_slice()
    }

    fn stdin_text(&self) -> Result<String, String> {
        let mut state = self
            .stdin
            .lock()
            .expect("stdin buffer mutex should not be poisoned");
        match &*state {
            StdinState::Loaded(result) => result.clone(),
            StdinState::Provided(text) => {
                let text = text.clone();
                *state = StdinState::Loaded(Ok(text.clone()));
                Ok(text)
            }
            StdinState::HostPending => {
                let mut text = String::new();
                let result = std::io::stdin()
                    .read_to_string(&mut text)
                    .map(|_| text)
                    .map_err(|error| error.to_string());
                *state = StdinState::Loaded(result.clone());
                result
            }
        }
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl TaskHandle {
    fn new(boundary: TaskBoundary) -> Self {
        Self {
            boundary,
            result: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    fn store(&self, outcome: TaskOutcome) {
        let (lock, ready) = &*self.result;
        let mut slot = lock
            .lock()
            .expect("task result mutex should not be poisoned");
        *slot = Some(outcome);
        ready.notify_all();
    }

    fn store_value(&self, value: Value) {
        self.store(TaskOutcome::Value(value));
    }

    fn store_diagnostics(&self, diagnostics: Diagnostics) {
        self.store(TaskOutcome::Diagnostics(diagnostics));
    }

    fn store_panic(&self, panic: TaskPanic) {
        self.store(TaskOutcome::Panic(panic));
    }

    fn wait_outcome(&self, token: Option<&CancelTokenValue>) -> Option<TaskOutcome> {
        let (lock, ready) = &*self.result;
        let mut slot = lock
            .lock()
            .expect("task result mutex should not be poisoned");
        loop {
            if let Some(outcome) = slot.as_ref() {
                return Some(outcome.clone());
            }
            if token.is_some_and(CancelTokenValue::is_cancelled) {
                return None;
            }
            if token.is_some() {
                let (next_slot, _) = ready
                    .wait_timeout(slot, Duration::from_millis(5))
                    .expect("task result wait_timeout should not be poisoned");
                slot = next_slot;
            } else {
                slot = ready
                    .wait(slot)
                    .expect("task result wait should not be poisoned");
            }
        }
    }

    fn await_value(&self) -> Result<Value, Diagnostics> {
        match self
            .wait_outcome(None)
            .expect("non-cancellable task waits should always produce an outcome")
        {
            TaskOutcome::Value(value) => Ok(value),
            TaskOutcome::Diagnostics(diagnostics) => match self.boundary {
                TaskBoundary::Direct => Err(diagnostics),
                TaskBoundary::RuntimeResult => Ok(result_err(runtime_task_failed_error(
                    task_failure_message(&diagnostics),
                ))),
            },
            TaskOutcome::Panic(panic) => match self.boundary {
                TaskBoundary::Direct => Err(task_panic_diagnostics(&panic)),
                TaskBoundary::RuntimeResult => Ok(result_err(runtime_task_panicked_error(
                    &panic.function_name,
                ))),
            },
        }
    }

    fn await_result_value(&self, token: Option<&CancelTokenValue>) -> Value {
        match self.wait_outcome(token) {
            None => result_err(runtime_cancelled_error()),
            Some(TaskOutcome::Value(value)) => result_ok(value),
            Some(TaskOutcome::Diagnostics(diagnostics)) => result_err(runtime_task_failed_error(
                task_failure_message(&diagnostics),
            )),
            Some(TaskOutcome::Panic(panic)) => {
                result_err(runtime_task_panicked_error(&panic.function_name))
            }
        }
    }
}

impl ChannelHandle {
    fn new(capacity: Option<usize>) -> Self {
        Self {
            state: Mutex::new(ChannelState {
                queue: VecDeque::new(),
                rendezvous_slot: None,
                closed: false,
                capacity,
                waiting_receivers: 0,
            }),
            ready: Condvar::new(),
        }
    }

    fn can_send(state: &ChannelState) -> bool {
        match state.capacity {
            None => true,
            Some(0) => state.rendezvous_slot.is_none(),
            Some(capacity) => state.queue.len() < capacity,
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

        let mut pending = Some(value);
        let mut rendezvous_enqueued = false;
        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        loop {
            if rendezvous_enqueued && state.rendezvous_slot.is_none() {
                return ChannelReceiveState::Value(Value::Unit);
            }
            if state.closed {
                if rendezvous_enqueued && state.rendezvous_slot.is_some() {
                    state.rendezvous_slot = None;
                    self.ready.notify_all();
                }
                return ChannelReceiveState::Closed;
            }
            if token.is_some_and(CancelTokenValue::is_cancelled) {
                if rendezvous_enqueued && state.rendezvous_slot.is_some() {
                    state.rendezvous_slot = None;
                    self.ready.notify_all();
                }
                return ChannelReceiveState::Cancelled;
            }
            match state.capacity {
                Some(0) => {
                    if !rendezvous_enqueued && Self::can_send(&state) {
                        state.rendezvous_slot = Some(
                            pending
                                .take()
                                .expect("pending rendezvous send value should exist"),
                        );
                        rendezvous_enqueued = true;
                        self.ready.notify_all();
                    }
                }
                _ => {
                    if Self::can_send(&state) {
                        state.queue.push_back(
                            pending
                                .take()
                                .expect("pending channel send value should exist"),
                        );
                        self.ready.notify_all();
                        return ChannelReceiveState::Value(Value::Unit);
                    }
                }
            }
            let (next_state, _) = self
                .ready
                .wait_timeout(state, Duration::from_millis(10))
                .expect("channel wait should not be poisoned");
            state = next_state;
        }
    }

    fn recv(&self, token: Option<&CancelTokenValue>) -> ChannelReceiveState {
        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        let mut waiting_registered = false;
        loop {
            if let Some(value) = state.queue.pop_front() {
                if waiting_registered {
                    state.waiting_receivers -= 1;
                }
                self.ready.notify_all();
                return ChannelReceiveState::Value(value);
            }
            if let Some(value) = state.rendezvous_slot.take() {
                if waiting_registered {
                    state.waiting_receivers -= 1;
                }
                self.ready.notify_all();
                return ChannelReceiveState::Value(value);
            }
            if state.closed {
                if waiting_registered {
                    state.waiting_receivers -= 1;
                    self.ready.notify_all();
                }
                return ChannelReceiveState::Closed;
            }
            if token.is_some_and(CancelTokenValue::is_cancelled) {
                if waiting_registered {
                    state.waiting_receivers -= 1;
                    self.ready.notify_all();
                }
                return ChannelReceiveState::Cancelled;
            }
            if matches!(state.capacity, Some(0)) && !waiting_registered {
                state.waiting_receivers += 1;
                waiting_registered = true;
                self.ready.notify_all();
            }
            let (next_state, _) = self
                .ready
                .wait_timeout(state, Duration::from_millis(10))
                .expect("channel wait should not be poisoned");
            state = next_state;
        }
    }

    fn try_send(
        &self,
        value: Value,
        token: Option<&CancelTokenValue>,
    ) -> Option<ChannelReceiveState> {
        if token.is_some_and(CancelTokenValue::is_cancelled) {
            return Some(ChannelReceiveState::Cancelled);
        }

        let mut state = self
            .state
            .lock()
            .expect("channel state mutex should not be poisoned");
        if state.closed {
            return Some(ChannelReceiveState::Closed);
        }

        match state.capacity {
            None => {
                state.queue.push_back(value);
                self.ready.notify_all();
                Some(ChannelReceiveState::Value(Value::Unit))
            }
            Some(0) => {
                if state.waiting_receivers > 0 && state.rendezvous_slot.is_none() {
                    state.rendezvous_slot = Some(value);
                    self.ready.notify_all();
                    Some(ChannelReceiveState::Value(Value::Unit))
                } else {
                    None
                }
            }
            Some(capacity) => {
                if state.queue.len() < capacity {
                    state.queue.push_back(value);
                    self.ready.notify_all();
                    Some(ChannelReceiveState::Value(Value::Unit))
                } else {
                    None
                }
            }
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
            self.ready.notify_all();
            Some(ChannelReceiveState::Value(value))
        } else if let Some(value) = state.rendezvous_slot.take() {
            self.ready.notify_all();
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

fn runtime_time_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Time",
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

fn runtime_task_failed_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "TaskFailed",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_task_panicked_error(task: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "TaskPanicked",
        vec![("task".to_string(), Value::String(task.into()))],
    )
}

fn runtime_parse_int_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "ParseInt",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_empty_sequence_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "EmptySequence",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_slice_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Slice",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_json_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Json",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_csv_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Csv",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_toml_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Toml",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_yaml_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Yaml",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_base64_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Base64",
        vec![("message".to_string(), Value::String(message.into()))],
    )
}

fn runtime_template_error(message: impl Into<String>) -> Value {
    enum_value(
        "RuntimeError",
        "Template",
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

fn unix_duration_since_epoch(now: SystemTime) -> Result<Duration, String> {
    now.duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))
}

fn unix_seconds_from_duration_since_epoch(duration: Duration) -> Result<i64, String> {
    if duration.as_secs() > i64::MAX as u64 {
        return Err("unix second timestamp exceeds bootstrap int range".to_string());
    }
    Ok(duration.as_secs() as i64)
}

fn unix_seconds_from_system_time(now: SystemTime) -> Result<i64, String> {
    let duration = unix_duration_since_epoch(now)?;
    unix_seconds_from_duration_since_epoch(duration)
}

fn unix_millis_from_duration_since_epoch(duration: Duration) -> Result<i64, String> {
    let millis = duration.as_millis();
    if millis > i64::MAX as u128 {
        return Err("unix millisecond timestamp exceeds bootstrap int range".to_string());
    }
    Ok(millis as i64)
}

fn unix_millis_from_system_time(now: SystemTime) -> Result<i64, String> {
    let duration = unix_duration_since_epoch(now)?;
    unix_millis_from_duration_since_epoch(duration)
}

fn task_failure_message(diagnostics: &Diagnostics) -> String {
    if diagnostics.0.is_empty() {
        return "spawned task failed before producing a value".to_string();
    }

    diagnostics
        .0
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn task_panic_diagnostics(panic: &TaskPanic) -> Diagnostics {
    Diagnostics(vec![
        Diagnostic::error(
            "GOF3010",
            format!("task `{}` panicked", panic.function_name),
            "a spawned task hit an internal failure before producing a value",
            panic.span,
        )
        .with_fix_it("inspect the spawned function and remove invariant-breaking panics")
        .with_source_path(panic.source_path.clone()),
    ])
}

fn function_returns_runtime_result(return_type: Option<&crate::ast::TypeRef>) -> bool {
    let Some(return_type) = return_type else {
        return false;
    };

    return_type.name == "Result"
        && return_type.args.len() == 2
        && return_type.args[1].name == "RuntimeError"
        && return_type.args[1].args.is_empty()
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
    run_with_output_with_args(module, host_program_args())
}

pub fn run_with_output_with_args(
    module: &Module,
    program_args: Vec<String>,
) -> Result<ExecutionResult, Diagnostics> {
    run_with_output_with_args_and_optional_stdin(module, program_args, None)
}

fn run_with_output_with_args_and_optional_stdin(
    module: &Module,
    program_args: Vec<String>,
    stdin: Option<String>,
) -> Result<ExecutionResult, Diagnostics> {
    reset_select_arm_rotation();

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
    let output = match stdin {
        Some(stdin) => OutputBuffer::new_with_stdin(program_args, stdin),
        None => OutputBuffer::new(program_args),
    };

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
        Stmt::Select { arms, span } => {
            if arms.is_empty() {
                return Err(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` requires at least one arm",
                        "the bootstrap select model needs one or more `recv(channel)`, `send(channel, value)`, or `default` arms",
                        *span,
                    )
                    .with_fix_it("add a select arm like `value = recv(ch):`, `send(ch, value):`, or `default:`")
                    .with_source_path(source_path.to_path_buf()),
                ])
                .into());
            }

            let mut prepared_arms = Vec::new();
            let mut default_arm: Option<&SelectArm> = None;
            for arm in arms {
                match arm.kind {
                    SelectArmKind::Operation { ref operation } => prepared_arms.push((
                        arm,
                        prepare_select_operation(
                            operation,
                            scopes,
                            functions,
                            methods,
                            structs,
                            enums,
                            output,
                            source_path,
                        )?,
                    )),
                    SelectArmKind::Default => {
                        if default_arm.is_some() {
                            return Err(Diagnostics(vec![
                                Diagnostic::error(
                                    "GOF3095",
                                    "`select` allows only one `default` arm",
                                    "multiple `default` arms would make the immediate fallback path ambiguous",
                                    arm.span,
                                )
                                .with_fix_it("remove the duplicate `default` arm or merge its body into the first one")
                                .with_source_path(source_path.to_path_buf()),
                            ])
                            .into());
                        }
                        default_arm = Some(arm);
                    }
                }
            }

            if prepared_arms.is_empty() {
                if let Some(arm) = default_arm {
                    return eval_select_arm_body(
                        arm,
                        None,
                        scopes,
                        functions,
                        methods,
                        structs,
                        enums,
                        output,
                        loop_depth,
                        source_path,
                    );
                }
            }

            let mut start_index = next_select_arm_start(prepared_arms.len());
            loop {
                for offset in 0..prepared_arms.len() {
                    let (arm, prepared) =
                        &prepared_arms[(start_index + offset) % prepared_arms.len()];
                    if let Some(received) = poll_select_operation(prepared) {
                        return eval_select_arm_body(
                            arm,
                            Some(received),
                            scopes,
                            functions,
                            methods,
                            structs,
                            enums,
                            output,
                            loop_depth,
                            source_path,
                        );
                    }
                }

                if let Some(arm) = default_arm {
                    return eval_select_arm_body(
                        arm,
                        None,
                        scopes,
                        functions,
                        methods,
                        structs,
                        enums,
                        output,
                        loop_depth,
                        source_path,
                    );
                }

                start_index = (start_index + 1) % prepared_arms.len();
                std::thread::yield_now();
            }
        }
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
            if let Some(function) = functions.get(callee).cloned() {
                let values = args
                    .iter()
                    .map(|arg| {
                        eval_expr(arg, scopes, functions, methods, structs, enums, output, source_path)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(eval_function(
                    &function, &values, functions, methods, structs, enums, output,
                )?);
            }

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

            if callee == "first" {
                return eval_first_builtin(
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

            if callee == "last" {
                return eval_last_builtin(
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

            if callee == "slice" {
                return eval_slice_builtin(
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

            if callee == "reverse" {
                return eval_reverse_builtin(
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

            if callee == "sort" {
                return eval_sort_builtin(
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

            if callee == "min" {
                return eval_min_builtin(
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

            if callee == "max" {
                return eval_max_builtin(
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
                return eval_argv_builtin(args, output, source_path, *span);
            }

            if callee == "read_stdin" {
                return eval_read_stdin_builtin(args, output, source_path, *span);
            }

            if callee == "read_stdin_lines" {
                return eval_read_stdin_lines_builtin(args, output, source_path, *span);
            }

            if callee == "unix_seconds" {
                return eval_unix_seconds_builtin(args, source_path, *span);
            }

            if callee == "unix_millis" {
                return eval_unix_millis_builtin(args, source_path, *span);
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

            if callee == "run_process" {
                return eval_run_process_builtin(
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

            if callee == "read_lines" {
                return eval_read_lines_builtin(
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

            if callee == "write_lines" {
                return eval_write_lines_builtin(
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
                return Ok(eval_channel_builtin(
                    args,
                    scopes,
                    functions,
                    methods,
                    structs,
                    enums,
                    output,
                    source_path,
                    *span,
                )?);
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

            if callee == "timeout_token" {
                return eval_timeout_token_builtin(
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

            if callee == "cancel_after" {
                return eval_cancel_after_builtin(
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

            if callee == "await_result" {
                return eval_await_result_builtin(
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

            if callee == "toml_parse" {
                return eval_toml_parse_builtin(
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

            if callee == "yaml_parse" {
                return eval_yaml_parse_builtin(
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

            if callee == "base64_encode" {
                return eval_base64_encode_builtin(
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

            if callee == "base64_decode" {
                return eval_base64_decode_builtin(
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

            if callee == "csv_parse" {
                return eval_csv_parse_builtin(
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

            if callee == "csv_stringify" {
                return eval_csv_stringify_builtin(
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

            if callee == "template_render" {
                return eval_template_render_builtin(
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

            if callee == "http_request" {
                return eval_http_request_builtin(
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

            Err(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3004",
                    format!("unknown function `{callee}`"),
                    "only top-level named functions can be called in the bootstrap evaluator",
                    *span,
                )
                .with_fix_it("define the function before calling it")
                .with_source_path(source_path.to_path_buf()),
            ])
            .into())
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

    let boundary = if function_returns_runtime_result(function.return_type.as_ref()) {
        TaskBoundary::RuntimeResult
    } else {
        TaskBoundary::Direct
    };
    let task = Arc::new(TaskHandle::new(boundary));
    let task_handle = Arc::clone(&task);
    let function_name = callee.to_string();
    let functions = Arc::clone(functions);
    let methods = Arc::clone(methods);
    let structs = Arc::clone(structs);
    let enums = Arc::clone(enums);
    let output = output.clone();

    std::thread::spawn(move || {
        match std::panic::catch_unwind(AssertUnwindSafe(|| {
            eval_function(
                &function, &args, &functions, &methods, &structs, &enums, &output,
            )
        })) {
            Ok(Ok(value)) => task_handle.store_value(value),
            Ok(Err(diagnostics)) => task_handle.store_diagnostics(diagnostics),
            Err(_) => task_handle.store_panic(TaskPanic {
                function_name,
                source_path: function.source_path.clone(),
                span,
            }),
        }
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
        (Value::String(lhs), BinaryOp::Lt, Value::String(rhs)) => Ok(Value::Bool(lhs < rhs)),
        (Value::String(lhs), BinaryOp::Le, Value::String(rhs)) => Ok(Value::Bool(lhs <= rhs)),
        (Value::String(lhs), BinaryOp::Gt, Value::String(rhs)) => Ok(Value::Bool(lhs > rhs)),
        (Value::String(lhs), BinaryOp::Ge, Value::String(rhs)) => Ok(Value::Bool(lhs >= rhs)),
        (Value::Bool(lhs), BinaryOp::Eq, Value::Bool(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Bool(lhs), BinaryOp::Ne, Value::Bool(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Json(lhs), BinaryOp::Eq, Value::Json(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Json(lhs), BinaryOp::Ne, Value::Json(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Struct(lhs), BinaryOp::Eq, Value::Struct(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Struct(lhs), BinaryOp::Ne, Value::Struct(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Enum(lhs), BinaryOp::Eq, Value::Enum(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Enum(lhs), BinaryOp::Ne, Value::Enum(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::List(lhs), BinaryOp::Eq, Value::List(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::List(lhs), BinaryOp::Ne, Value::List(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Dict(lhs), BinaryOp::Eq, Value::Dict(rhs)) => Ok(Value::Bool(lhs == rhs)),
        (Value::Dict(lhs), BinaryOp::Ne, Value::Dict(rhs)) => Ok(Value::Bool(lhs != rhs)),
        (Value::Unit, BinaryOp::Eq, Value::Unit) => Ok(Value::Bool(true)),
        (Value::Unit, BinaryOp::Ne, Value::Unit) => Ok(Value::Bool(false)),
        _ => eval_diagnostics(Diagnostics(vec![Diagnostic::error(
            "GOF3001",
            "unsupported expression in bootstrap evaluator",
            "the current evaluator supports int arithmetic, lexicographic string ordering, and equality for bool, json, unit, and structural string/list/dict/struct/enum/result values",
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

fn eval_first_builtin(
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
                "wrong number of arguments for `first`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `first(list_value)`")
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

    let Value::List(values) = list_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`first` requires a list value",
                format!("this argument resolves to `{}`", value_name(&list_value)),
                args[0].span(),
            )
            .with_fix_it("pass a list value to `first`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match values.first() {
        Some(value) => result_ok(value.clone()),
        None => result_err(runtime_empty_sequence_error(
            "`first` requires a non-empty list",
        )),
    })
}

fn eval_last_builtin(
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
                "wrong number of arguments for `last`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `last(list_value)`")
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

    let Value::List(values) = list_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`last` requires a list value",
                format!("this argument resolves to `{}`", value_name(&list_value)),
                args[0].span(),
            )
            .with_fix_it("pass a list value to `last`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match values.last() {
        Some(value) => result_ok(value.clone()),
        None => result_err(runtime_empty_sequence_error(
            "`last` requires a non-empty list",
        )),
    })
}

fn eval_slice_builtin(
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
                "wrong number of arguments for `slice`",
                format!("expected 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `slice(list_value, start, end)`")
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
    let start = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let end = eval_expr(
        &args[2],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    let Value::List(values) = list_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`slice` requires a list as its first argument",
                format!("this argument resolves to `{}`", value_name(&list_value)),
                args[0].span(),
            )
            .with_fix_it("pass a list value as the first argument to `slice`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::Int(start) = start else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`slice` argument 2 must resolve to `int`",
                format!("this argument resolves to `{}`", value_name(&start)),
                args[1].span(),
            )
            .with_fix_it("pass integer start and end indexes to `slice`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::Int(end) = end else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`slice` argument 3 must resolve to `int`",
                format!("this argument resolves to `{}`", value_name(&end)),
                args[2].span(),
            )
            .with_fix_it("pass integer start and end indexes to `slice`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if start < 0 || end < 0 {
        return Ok(result_err(runtime_slice_error(
            "`slice` indexes must be non-negative",
        )));
    }
    if start > end {
        return Ok(result_err(runtime_slice_error(
            "`slice` start index cannot exceed end index",
        )));
    }

    let len = values.len() as i64;
    if end > len {
        return Ok(result_err(runtime_slice_error(format!(
            "`slice` end index {end} exceeds list length {len}"
        ))));
    }

    Ok(result_ok(Value::List(
        values[start as usize..end as usize].to_vec(),
    )))
}

fn eval_reverse_builtin(
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
                "wrong number of arguments for `reverse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `reverse(list_value)`")
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

    let Value::List(mut values) = list_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                "`reverse` requires a list value",
                format!("this argument resolves to `{}`", value_name(&list_value)),
                args[0].span(),
            )
            .with_fix_it("pass a list value to `reverse`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    values.reverse();
    Ok(Value::List(values))
}

fn eval_sort_builtin(
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
    let mut values = eval_orderable_list_builtin(
        "sort",
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

    if values.iter().all(|value| matches!(value, Value::Int(_))) {
        values.sort_by(|lhs, rhs| match (lhs, rhs) {
            (Value::Int(lhs), Value::Int(rhs)) => lhs.cmp(rhs),
            _ => unreachable!("all values should be ints after validation"),
        });
        return Ok(Value::List(values));
    }

    if values.iter().all(|value| matches!(value, Value::String(_))) {
        values.sort_by(|lhs, rhs| match (lhs, rhs) {
            (Value::String(lhs), Value::String(rhs)) => lhs.cmp(rhs),
            _ => unreachable!("all values should be strings after validation"),
        });
        return Ok(Value::List(values));
    }

    if values.is_empty() {
        return Ok(Value::List(values));
    }

    eval_diagnostics(Diagnostics(vec![
        Diagnostic::error(
            "GOF3088",
            "`sort` currently requires `list[int]` or `list[string]`",
            "the runtime sort baseline supports only homogeneous int or string lists",
            args[0].span(),
        )
        .with_fix_it("sort a list of ints or strings")
        .with_source_path(source_path.to_path_buf()),
    ]))
}

fn eval_min_builtin(
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
    let values = eval_orderable_list_builtin(
        "min",
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

    if values.is_empty() {
        return Ok(result_err(runtime_empty_sequence_error(
            "`min` requires a non-empty list",
        )));
    }

    if values.iter().all(|value| matches!(value, Value::Int(_))) {
        let min_value = values
            .iter()
            .min_by(|lhs, rhs| match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => lhs.cmp(rhs),
                _ => unreachable!("all values should be ints after validation"),
            })
            .expect("non-empty int list should have a minimum")
            .clone();
        return Ok(result_ok(min_value));
    }

    let min_value = values
        .iter()
        .min_by(|lhs, rhs| match (lhs, rhs) {
            (Value::String(lhs), Value::String(rhs)) => lhs.cmp(rhs),
            _ => unreachable!("all values should be strings after validation"),
        })
        .expect("non-empty string list should have a minimum")
        .clone();
    Ok(result_ok(min_value))
}

fn eval_max_builtin(
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
    let values = eval_orderable_list_builtin(
        "max",
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

    if values.is_empty() {
        return Ok(result_err(runtime_empty_sequence_error(
            "`max` requires a non-empty list",
        )));
    }

    if values.iter().all(|value| matches!(value, Value::Int(_))) {
        let max_value = values
            .iter()
            .max_by(|lhs, rhs| match (lhs, rhs) {
                (Value::Int(lhs), Value::Int(rhs)) => lhs.cmp(rhs),
                _ => unreachable!("all values should be ints after validation"),
            })
            .expect("non-empty int list should have a maximum")
            .clone();
        return Ok(result_ok(max_value));
    }

    let max_value = values
        .iter()
        .max_by(|lhs, rhs| match (lhs, rhs) {
            (Value::String(lhs), Value::String(rhs)) => lhs.cmp(rhs),
            _ => unreachable!("all values should be strings after validation"),
        })
        .expect("non-empty string list should have a maximum")
        .clone();
    Ok(result_ok(max_value))
}

fn eval_orderable_list_builtin(
    callee: &'static str,
    args: &[Expr],
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Vec<Value>> {
    if args.len() != 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{callee}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{callee}(list_value)`"))
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

    let Value::List(values) = list_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3088",
                format!("`{callee}` requires a list value"),
                format!("this argument resolves to `{}`", value_name(&list_value)),
                args[0].span(),
            )
            .with_fix_it(format!("pass a list value to `{callee}`"))
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    if values.is_empty()
        || values.iter().all(|value| matches!(value, Value::Int(_)))
        || values.iter().all(|value| matches!(value, Value::String(_)))
    {
        return Ok(values);
    }

    eval_diagnostics(Diagnostics(vec![
        Diagnostic::error(
            "GOF3088",
            format!("`{callee}` currently requires `list[int]` or `list[string]`"),
            format!(
                "the runtime `{callee}` baseline supports only homogeneous int or string lists"
            ),
            args[0].span(),
        )
        .with_fix_it(format!("call `{callee}` on a list of ints or strings"))
        .with_source_path(source_path.to_path_buf()),
    ]))
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

    Ok(match value_text.parse::<i64>() {
        Ok(value) => result_ok(Value::Int(value)),
        Err(error) => result_err(runtime_parse_int_error(format!(
            "failed to parse int from `{value_text}`: {error}"
        ))),
    })
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

fn eval_argv_builtin(
    args: &[Expr],
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
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
        output
            .program_args()
            .iter()
            .cloned()
            .map(Value::String)
            .collect(),
    ))
}

fn eval_read_stdin_builtin(
    args: &[Expr],
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `read_stdin`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_stdin()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(match output.stdin_text() {
        Ok(text) => result_ok(Value::String(text)),
        Err(error) => result_err(runtime_io_error(error)),
    })
}

fn eval_read_stdin_lines_builtin(
    args: &[Expr],
    output: &OutputBuffer,
    source_path: &Path,
    span: Span,
) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `read_stdin_lines`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_stdin_lines()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(match output.stdin_text() {
        Ok(text) => result_ok(Value::List(collect_text_lines(&text))),
        Err(error) => result_err(runtime_io_error(error)),
    })
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

fn eval_unix_seconds_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `unix_seconds`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `unix_seconds()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(match unix_seconds_from_system_time(SystemTime::now()) {
        Ok(seconds) => result_ok(Value::Int(seconds)),
        Err(message) => result_err(runtime_time_error(message)),
    })
}

fn eval_unix_millis_builtin(args: &[Expr], source_path: &Path, span: Span) -> EvalResult<Value> {
    if !args.is_empty() {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `unix_millis`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `unix_millis()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(match unix_millis_from_system_time(SystemTime::now()) {
        Ok(millis) => result_ok(Value::Int(millis)),
        Err(message) => result_err(runtime_time_error(message)),
    })
}

fn eval_run_process_builtin(
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
                "wrong number of arguments for `run_process`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `run_process(program, args)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let program = eval_string_argument(
        "run_process",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3100",
    )?;
    let program_display = program.clone();
    let argument_strings = eval_string_list_argument(
        "run_process",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3100",
    )?;

    let command_output = Command::new(&program)
        .args(&argument_strings)
        .output()
        .map_err(|error| runtime_io_error(format!("failed to run `{program_display}`: {error}")));

    Ok(match command_output {
        Ok(output) => {
            let Some(status) = output.status.code() else {
                return Ok(result_err(runtime_io_error(format!(
                    "process `{program_display}` exited without an integer status"
                ))));
            };

            let mut report = BTreeMap::new();
            report.insert("program".to_string(), JsonValue::String(program));
            report.insert(
                "args".to_string(),
                JsonValue::Array(
                    argument_strings
                        .into_iter()
                        .map(JsonValue::String)
                        .collect::<Vec<_>>(),
                ),
            );
            report.insert("status".to_string(), JsonValue::Int(i64::from(status)));
            report.insert(
                "stdout".to_string(),
                JsonValue::String(String::from_utf8_lossy(&output.stdout).into_owned()),
            );
            report.insert(
                "stderr".to_string(),
                JsonValue::String(String::from_utf8_lossy(&output.stderr).into_owned()),
            );
            result_ok(Value::Json(JsonValue::Object(report)))
        }
        Err(error) => result_err(error),
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

fn eval_http_headers_argument(
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
) -> EvalResult<BTreeMap<String, String>> {
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
    let Value::Dict(headers) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                diagnostic_code,
                format!("`{builtin_name}` requires a `dict[string]` headers argument"),
                format!("this argument resolves to `{}`", value_name(&value)),
                expr.span(),
            )
            .with_fix_it(
                "pass a dict of string header values like `{\"Accept\": \"application/json\"}`",
            )
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let mut rendered_headers = BTreeMap::new();
    let mut diagnostics = Diagnostics::default();
    for (name, value) in headers.entries {
        match value {
            Value::String(text) => {
                rendered_headers.insert(name, text);
            }
            other => diagnostics.push(
                Diagnostic::error(
                    diagnostic_code,
                    format!("`{builtin_name}` requires string header values"),
                    format!("header `{name}` resolves to `{}`", value_name(&other)),
                    expr.span(),
                )
                .with_fix_it("ensure every header value is a string")
                .with_source_path(source_path.to_path_buf()),
            ),
        }
    }

    if !diagnostics.is_empty() {
        return eval_diagnostics(diagnostics);
    }

    Ok(rendered_headers)
}

fn eval_string_list_argument(
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
) -> EvalResult<Vec<String>> {
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
    let Value::List(items) = value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                diagnostic_code,
                format!("`{builtin_name}` requires `list[string]` arguments"),
                format!("this value resolves to `{}`", value_name(&value)),
                expr.span(),
            )
            .with_fix_it(format!("pass a `list[string]` value to `{builtin_name}`"))
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let mut strings = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(text) = item else {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    diagnostic_code,
                    format!("`{builtin_name}` requires `list[string]` arguments"),
                    format!("this list contains `{}`", value_name(&item)),
                    expr.span(),
                )
                .with_fix_it(format!(
                    "ensure every argument passed to `{builtin_name}` is a string"
                ))
                .with_source_path(source_path.to_path_buf()),
            ]));
        };
        strings.push(text);
    }

    Ok(strings)
}

fn collect_text_lines(contents: &str) -> Vec<Value> {
    contents
        .lines()
        .map(|line| Value::String(line.to_string()))
        .collect()
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

fn eval_non_negative_duration_argument(
    builtin_name: &str,
    expr: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
    invalid_operand_code: &'static str,
    negative_duration_code: &'static str,
) -> EvalResult<i64> {
    let millis = eval_int_argument(
        builtin_name,
        expr,
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        invalid_operand_code,
    )?;

    if millis < 0 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                negative_duration_code,
                format!("`{builtin_name}` requires a non-negative duration"),
                format!("this duration resolves to `{millis}`"),
                expr.span(),
            )
            .with_fix_it("pass `0` or another non-negative millisecond count")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    Ok(millis)
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

fn eval_read_lines_builtin(
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
                "wrong number of arguments for `read_lines`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_lines(path)`")
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
                "`read_lines` requires a string path",
                format!("this path resolves to `{}`", value_name(&path_value)),
                args[0].span(),
            )
            .with_fix_it("pass a string path like `\"notes.txt\"` to `read_lines`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    Ok(match fs::read_to_string(&path_text) {
        Ok(contents) => result_ok(Value::List(collect_text_lines(&contents))),
        Err(error) => result_err(runtime_io_error(error.to_string())),
    })
}

fn eval_write_lines_builtin(
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
                "wrong number of arguments for `write_lines`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `write_lines(path, lines)`")
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
    let lines_value = eval_expr(
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
                "`write_lines` requires a string path",
                format!("this path resolves to `{}`", value_name(&path_value)),
                args[0].span(),
            )
            .with_fix_it("pass a string path as the first argument to `write_lines`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };
    let Value::List(lines) = lines_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3043",
                "`write_lines` requires `list[string]` contents",
                format!(
                    "this contents value resolves to `{}`",
                    value_name(&lines_value)
                ),
                args[1].span(),
            )
            .with_fix_it("pass a `list[string]` value as the second argument to `write_lines`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let mut string_lines = Vec::with_capacity(lines.len());
    for line in lines {
        let Value::String(text) = line else {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3043",
                    "`write_lines` requires `list[string]` contents",
                    format!("this list contains `{}`", value_name(&line)),
                    args[1].span(),
                )
                .with_fix_it("ensure every element passed to `write_lines` is a string")
                .with_source_path(source_path.to_path_buf()),
            ]));
        };
        string_lines.push(text);
    }

    Ok(match fs::write(&path_text, string_lines.join("\n")) {
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

fn eval_channel_builtin(
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
    if args.len() > 1 {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `channel`",
                format!("expected 0 or 1 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `channel()` or `channel(capacity)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let capacity = if let Some(capacity_expr) = args.first() {
        let capacity = eval_int_argument(
            "channel",
            capacity_expr,
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
            "GOF3096",
        )?;
        if capacity < 0 {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3097",
                    "`channel` requires a non-negative capacity",
                    format!("this capacity resolves to `{capacity}`"),
                    capacity_expr.span(),
                )
                .with_fix_it("pass `0` or another non-negative channel capacity")
                .with_source_path(source_path.to_path_buf()),
            ]));
        }
        Some(capacity as usize)
    } else {
        None
    };

    Ok(Value::Channel(ChannelValue(Arc::new(ChannelHandle::new(
        capacity,
    )))))
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

#[derive(Clone)]
enum PreparedSelectOperation {
    Recv {
        channel: ChannelValue,
        cancel_token: Option<CancelTokenValue>,
    },
    Send {
        channel: ChannelValue,
        value: Value,
        cancel_token: Option<CancelTokenValue>,
    },
}

fn prepare_select_operation(
    operation: &Expr,
    scopes: &ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    source_path: &Path,
) -> EvalResult<PreparedSelectOperation> {
    match operation {
        Expr::Call { callee, args, span } if callee == "recv" => {
            if !(1..=2).contains(&args.len()) {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                        "select receive arms must call `recv` with one channel argument and an optional cancellation token",
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
            let Value::Channel(channel) = channel_value else {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` receive arms require a channel argument",
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

            Ok(PreparedSelectOperation::Recv {
                channel,
                cancel_token,
            })
        }
        Expr::Call { callee, args, span } if callee == "send" => {
            if !(2..=3).contains(&args.len()) {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                        "select send arms must call `send` with a channel, a value, and an optional cancellation token",
                        *span,
                    )
                    .with_fix_it("rewrite the arm as `send(channel, value):`, `send(channel, value, token):`, `result = send(channel, value):`, or `result = send(channel, value, token):`")
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
            let value = eval_expr(
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
            let Value::Channel(channel) = channel_value else {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3047",
                        "`select` send arms require a channel argument",
                        format!(
                            "this arm resolves to `{}` instead of a channel",
                            value_name(&channel_value)
                        ),
                        args[0].span(),
                    )
                    .with_fix_it("pass a channel value as the first argument to `send` inside the select arm")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            };

            Ok(PreparedSelectOperation::Send {
                channel,
                value,
                cancel_token,
            })
        }
        Expr::Call { callee, .. } => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                format!("this arm uses `{callee}(...)` instead"),
                operation.span(),
            )
            .with_fix_it("replace the arm operation with `recv(...)`, `send(...)`, or `default`")
            .with_source_path(source_path.to_path_buf()),
        ])),
        _ => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                "select arms must be written as `recv(channel):`, `recv(channel, token):`, `send(channel, value):`, `send(channel, value, token):`, `value = recv(channel):`, `value = recv(channel, token):`, `value = send(channel, value):`, `value = send(channel, value, token):`, or `default:`",
                operation.span(),
            )
            .with_fix_it("replace this arm with `recv(...)`, `send(...)`, or `default`")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
}

fn poll_select_operation(operation: &PreparedSelectOperation) -> Option<Value> {
    match operation {
        PreparedSelectOperation::Recv {
            channel,
            cancel_token,
        } => channel
            .try_recv(cancel_token.as_ref())
            .map(|state| match state {
                ChannelReceiveState::Value(value) => result_ok(value),
                ChannelReceiveState::Closed => result_err(runtime_channel_closed_error()),
                ChannelReceiveState::Cancelled => result_err(runtime_cancelled_error()),
            }),
        PreparedSelectOperation::Send {
            channel,
            value,
            cancel_token,
        } => channel
            .try_send(value.clone(), cancel_token.as_ref())
            .map(|state| match state {
                ChannelReceiveState::Value(_) => result_ok(Value::Unit),
                ChannelReceiveState::Closed => result_err(runtime_channel_closed_error()),
                ChannelReceiveState::Cancelled => result_err(runtime_cancelled_error()),
            }),
    }
}

fn eval_select_arm_body(
    arm: &SelectArm,
    binding_value: Option<Value>,
    scopes: &mut ScopeStack,
    functions: &FunctionTable,
    methods: &MethodTable,
    structs: &StructTable,
    enums: &EnumTable,
    output: &OutputBuffer,
    loop_depth: usize,
    source_path: &Path,
) -> EvalResult<EvalOutcome> {
    scopes.push();
    if let (Some(binding), Some(value)) = (&arm.binding, binding_value) {
        scopes.define_current(
            binding.clone(),
            Binding {
                mutable: false,
                value,
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
    Ok(result)
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

fn eval_timeout_token_builtin(
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
                "wrong number of arguments for `timeout_token`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `timeout_token(milliseconds)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let millis = eval_non_negative_duration_argument(
        "timeout_token",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3093",
        "GOF3094",
    )?;

    Ok(Value::CancelToken(CancelTokenValue::timeout_after(millis)))
}

fn eval_cancel_after_builtin(
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
                "wrong number of arguments for `cancel_after`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cancel_after(token, milliseconds)`")
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
    let millis = eval_non_negative_duration_argument(
        "cancel_after",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3093",
        "GOF3094",
    )?;
    token.cancel_after(millis);
    Ok(Value::Unit)
}

fn eval_await_result_builtin(
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
                "wrong number of arguments for `await_result`",
                format!("expected 1 or 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `await_result(task)` or `await_result(task, token)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let task_value = eval_expr(
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

    match task_value {
        Value::Task(task) => Ok(task.await_result_value(cancel_token.as_ref())),
        _ => eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3009",
                "`await_result` requires a task value",
                "only values produced by `go` can currently be joined recoverably in the bootstrap evaluator",
                span,
            )
            .with_fix_it("pass a value produced by `go`")
            .with_source_path(source_path.to_path_buf()),
        ])),
    }
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

fn eval_toml_parse_builtin(
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
                "wrong number of arguments for `toml_parse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `toml_parse(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "toml_parse",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3099",
    )?;
    Ok(match toml::from_str::<TomlValue>(&text) {
        Ok(value) => match json_from_toml(value) {
            Ok(value) => result_ok(Value::Json(value)),
            Err(message) => result_err(runtime_toml_error(message)),
        },
        Err(error) => result_err(runtime_toml_error(error.to_string())),
    })
}

fn eval_yaml_parse_builtin(
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
                "wrong number of arguments for `yaml_parse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `yaml_parse(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "yaml_parse",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3104",
    )?;
    Ok(match serde_yaml::from_str::<YamlValue>(&text) {
        Ok(value) => match json_from_yaml(value) {
            Ok(value) => result_ok(Value::Json(value)),
            Err(message) => result_err(runtime_yaml_error(message)),
        },
        Err(error) => result_err(runtime_yaml_error(error.to_string())),
    })
}

fn eval_base64_encode_builtin(
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
                "wrong number of arguments for `base64_encode`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `base64_encode(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "base64_encode",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3105",
    )?;
    Ok(Value::String(BASE64_STANDARD.encode(text.as_bytes())))
}

fn eval_base64_decode_builtin(
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
                "wrong number of arguments for `base64_decode`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `base64_decode(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "base64_decode",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3105",
    )?;
    Ok(match BASE64_STANDARD.decode(text.as_bytes()) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(value) => result_ok(Value::String(value)),
            Err(error) => result_err(runtime_base64_error(format!(
                "decoded base64 bytes are not valid UTF-8: {error}"
            ))),
        },
        Err(error) => result_err(runtime_base64_error(error.to_string())),
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

fn eval_csv_parse_builtin(
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
                "wrong number of arguments for `csv_parse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `csv_parse(text)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let text = eval_string_argument(
        "csv_parse",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3098",
    )?;

    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        match record {
            Ok(record) => rows.push(Value::List(
                record
                    .iter()
                    .map(|field| Value::String(field.to_string()))
                    .collect(),
            )),
            Err(error) => return Ok(result_err(runtime_csv_error(error.to_string()))),
        }
    }

    Ok(result_ok(Value::List(rows)))
}

fn eval_csv_stringify_builtin(
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
                "wrong number of arguments for `csv_stringify`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `csv_stringify(rows)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let rows_value = eval_expr(
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;
    let Value::List(rows) = rows_value else {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3098",
                "`csv_stringify` requires `list[list[string]]` rows",
                format!("this argument resolves to `{}`", value_name(&rows_value)),
                args[0].span(),
            )
            .with_fix_it("pass a `list[list[string]]` value to `csv_stringify`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    };

    let mut writer = WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    for row in rows {
        let Value::List(fields) = row else {
            return eval_diagnostics(Diagnostics(vec![
                Diagnostic::error(
                    "GOF3098",
                    "`csv_stringify` requires `list[list[string]]` rows",
                    format!("this rows value contains `{}`", value_name(&row)),
                    args[0].span(),
                )
                .with_fix_it("ensure every row passed to `csv_stringify` is a `list[string]`")
                .with_source_path(source_path.to_path_buf()),
            ]));
        };

        let mut string_fields = Vec::with_capacity(fields.len());
        for field in fields {
            let Value::String(field_text) = field else {
                return eval_diagnostics(Diagnostics(vec![
                    Diagnostic::error(
                        "GOF3098",
                        "`csv_stringify` requires `list[list[string]]` rows",
                        format!("this row contains `{}`", value_name(&field)),
                        args[0].span(),
                    )
                    .with_fix_it("ensure every CSV field passed to `csv_stringify` is a string")
                    .with_source_path(source_path.to_path_buf()),
                ]));
            };
            string_fields.push(field_text);
        }

        if let Err(error) = writer.write_record(&string_fields) {
            return Ok(result_err(runtime_csv_error(error.to_string())));
        }
    }

    let bytes = match writer.into_inner() {
        Ok(bytes) => bytes,
        Err(error) => return Ok(result_err(runtime_csv_error(error.to_string()))),
    };
    let csv_text = String::from_utf8(bytes)
        .expect("csv writer should always produce valid UTF-8 from string fields");
    Ok(result_ok(Value::String(csv_text)))
}

fn eval_template_render_builtin(
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
                "wrong number of arguments for `template_render`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `template_render(template, values)`")
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let template = eval_string_argument(
        "template_render",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3103",
    )?;
    let values = eval_expr(
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
    )?;

    Ok(match render_template_text(&template, &values) {
        Ok(rendered) => result_ok(Value::String(rendered)),
        Err(message) => result_err(runtime_template_error(message)),
    })
}

fn render_template_text(template: &str, values: &Value) -> Result<String, String> {
    let mut rendered = String::new();
    let mut remainder = template;

    while let Some(start) = remainder.find("{{") {
        rendered.push_str(&remainder[..start]);
        let placeholder = &remainder[start + 2..];
        let Some(end) = placeholder.find("}}") else {
            return Err(
                "template placeholder starting with `{{` must be closed with `}}`".to_string(),
            );
        };

        let key = placeholder[..end].trim();
        if key.is_empty() {
            return Err("template placeholders cannot be empty".to_string());
        }
        if key.contains('{') || key.contains('}') {
            return Err(format!(
                "template placeholder `{key}` cannot contain nested braces"
            ));
        }

        rendered.push_str(&template_context_value(values, key)?);
        remainder = &placeholder[end + 2..];
    }

    rendered.push_str(remainder);
    Ok(rendered)
}

fn template_context_value(values: &Value, key: &str) -> Result<String, String> {
    match values {
        Value::Dict(dict) => dict
            .get(key)
            .ok_or_else(|| format!("template key `{key}` is missing from the dict context"))
            .and_then(|value| render_template_value(value, key)),
        Value::Json(JsonValue::Object(entries)) => entries
            .get(key)
            .map(render_json_template_value)
            .ok_or_else(|| format!("template key `{key}` is missing from the json object")),
        Value::Json(other) => Err(format!(
            "template_render expects a top-level json object context, got `{}`",
            json_template_type_name(other)
        )),
        other => Err(format!(
            "template_render expects `dict[...]` or `json`, got `{}`",
            value_name(other)
        )),
    }
}

fn render_template_value(value: &Value, key: &str) -> Result<String, String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Int(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Json(value) => Ok(render_json_template_value(value)),
        Value::Struct(_) | Value::Enum(_) | Value::List(_) | Value::Dict(_) => value
            .cli_text()
            .ok_or_else(|| format!("template key `{key}` cannot be rendered as text")),
        Value::Channel(_) | Value::CancelToken(_) | Value::Task(_) | Value::Unit => Err(format!(
            "template key `{key}` resolves to non-printable `{}`",
            value_name(value)
        )),
    }
}

fn render_json_template_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Int(value) => value.to_string(),
        JsonValue::String(value) => value.clone(),
        JsonValue::Array(_) | JsonValue::Object(_) => value.cli_text(),
    }
}

fn json_template_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Int(_) => "int",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

struct HttpRequestSpec {
    method: String,
    url: String,
    body: Option<String>,
    headers: BTreeMap<String, String>,
    timeout_ms: u64,
}

fn eval_http_request_builtin(
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
    if !(2..=5).contains(&args.len()) {
        return eval_diagnostics(Diagnostics(vec![
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `http_request`",
                format!("expected 2 to 5 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it(
                "call `http_request(method, url)` or `http_request(method, url, body, headers, timeout_ms)`",
            )
            .with_source_path(source_path.to_path_buf()),
        ]));
    }

    let method = eval_string_argument(
        "http_request",
        &args[0],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3106",
    )?;
    let url = eval_string_argument(
        "http_request",
        &args[1],
        scopes,
        functions,
        methods,
        structs,
        enums,
        output,
        source_path,
        "GOF3106",
    )?;
    let body = if args.len() >= 3 {
        Some(eval_string_argument(
            "http_request",
            &args[2],
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
            "GOF3106",
        )?)
    } else {
        None
    };
    let headers = if args.len() >= 4 {
        eval_http_headers_argument(
            "http_request",
            &args[3],
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
            "GOF3106",
        )?
    } else {
        BTreeMap::new()
    };
    let timeout_ms = if args.len() == 5 {
        eval_non_negative_duration_argument(
            "http_request",
            &args[4],
            scopes,
            functions,
            methods,
            structs,
            enums,
            output,
            source_path,
            "GOF3107",
            "GOF3107",
        )? as u64
    } else {
        35_000
    };

    let spec = HttpRequestSpec {
        method,
        url,
        body,
        headers,
        timeout_ms,
    };
    Ok(eval_http_request_report(execute_http_request(&spec), &spec))
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

    let spec = HttpRequestSpec {
        method: "GET".to_string(),
        url,
        body: None,
        headers: BTreeMap::new(),
        timeout_ms: 35_000,
    };
    Ok(eval_http_text_response(execute_http_request(&spec)))
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
    let mut headers = BTreeMap::new();
    headers.insert("Content-Type".to_string(), content_type);
    let spec = HttpRequestSpec {
        method: "POST".to_string(),
        url,
        body: Some(body),
        headers,
        timeout_ms: 35_000,
    };
    Ok(eval_http_text_response(execute_http_request(&spec)))
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

    let millis = eval_non_negative_duration_argument(
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
        "GOF3086",
    )?;

    std::thread::sleep(Duration::from_millis(millis as u64));
    Ok(Value::Unit)
}

fn execute_http_request(spec: &HttpRequestSpec) -> Result<ureq::Response, ureq::Error> {
    let mut request = ureq::request(&spec.method, &spec.url);
    for (name, value) in &spec.headers {
        request = request.set(name, value);
    }
    request = request.timeout(Duration::from_millis(spec.timeout_ms));
    if let Some(body) = &spec.body {
        request.send_string(body)
    } else {
        request.call()
    }
}

fn eval_http_text_response(response: Result<ureq::Response, ureq::Error>) -> Value {
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

fn eval_http_request_report(
    response: Result<ureq::Response, ureq::Error>,
    spec: &HttpRequestSpec,
) -> Value {
    match response {
        Ok(response) => {
            let status = i64::from(response.status());
            let headers = http_response_headers_json(&response);
            match response.into_string() {
                Ok(body) => result_ok(Value::Json(http_response_report_json(
                    spec, status, body, headers,
                ))),
                Err(error) => result_err(runtime_http_request_error(error.to_string())),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let status = i64::from(code);
            let headers = http_response_headers_json(&response);
            let body = response.into_string().unwrap_or_else(|_| String::new());
            result_ok(Value::Json(http_response_report_json(
                spec, status, body, headers,
            )))
        }
        Err(ureq::Error::Transport(error)) => {
            result_err(runtime_http_request_error(error.to_string()))
        }
    }
}

fn http_response_headers_json(response: &ureq::Response) -> JsonValue {
    let mut names = response.headers_names();
    names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|lhs, rhs| lhs.eq_ignore_ascii_case(rhs));

    let mut headers = BTreeMap::new();
    for name in names {
        let values = response
            .all(&name)
            .into_iter()
            .map(|value| JsonValue::String(value.to_string()))
            .collect();
        headers.insert(name.to_ascii_lowercase(), JsonValue::Array(values));
    }
    JsonValue::Object(headers)
}

fn http_response_report_json(
    spec: &HttpRequestSpec,
    status: i64,
    body: String,
    headers: JsonValue,
) -> JsonValue {
    let mut report = BTreeMap::new();
    report.insert("method".to_string(), JsonValue::String(spec.method.clone()));
    report.insert("url".to_string(), JsonValue::String(spec.url.clone()));
    report.insert("status".to_string(), JsonValue::Int(status));
    report.insert("body".to_string(), JsonValue::String(body));
    report.insert("headers".to_string(), headers);
    JsonValue::Object(report)
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
    use super::{
        CancelTokenValue, ChannelHandle, ChannelReceiveState, TaskBoundary, TaskHandle, TaskPanic,
        Value, result_ok, run, run_with_output, run_with_output_with_args,
    };
    use crate::ast::parse;
    use crate::cst::CstModule;
    use crate::lexer::lex;
    use crate::source::SourceFile;
    use crate::source::Span;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, UNIX_EPOCH};
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

    fn run_source_with_output_and_args(
        text: &str,
        args: &[&str],
    ) -> Result<super::ExecutionResult, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("test.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        run_with_output_with_args(
            &module,
            args.iter().map(|value| (*value).to_string()).collect(),
        )
    }

    fn run_source_with_output_and_args_and_stdin(
        text: &str,
        args: &[&str],
        stdin: &str,
    ) -> Result<super::ExecutionResult, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("test.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let module = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        super::run_with_output_with_args_and_optional_stdin(
            &module,
            args.iter().map(|value| (*value).to_string()).collect(),
            Some(stdin.to_string()),
        )
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
    fn argv_uses_explicit_program_args_for_the_execution_context() {
        let result = run_source_with_output_and_args(
            "fn main() -> int:\n    values = argv()\n    return len(values)\n",
            &["alpha", "beta", "gamma"],
        )
        .expect("program should run");

        assert_eq!(result.value, Value::Int(3));
    }

    #[test]
    fn read_stdin_helpers_use_the_explicit_execution_context() {
        let result = run_source_with_output_and_args_and_stdin(
            "fn main() -> Result[string, RuntimeError]:\n    text = read_stdin()?\n    lines = read_stdin_lines()?\n    headline = first(lines)?\n    return template_render(\"chars={{chars}} first={{first}} lines={{lines}}\", {\"chars\": to_string(len(text)), \"first\": headline, \"lines\": to_string(len(lines))})\n",
            &[],
            "alpha\nbeta\n",
        )
        .expect("program should run");

        assert_eq!(
            result.value.cli_text().as_deref(),
            Some("Result.Ok(value: chars=11 first=alpha lines=2)")
        );
    }

    #[test]
    fn read_stdin_helpers_return_empty_results_for_empty_input() {
        let result = run_source_with_output_and_args_and_stdin(
            "fn main() -> Result[int, RuntimeError]:\n    text = read_stdin()?\n    lines = read_stdin_lines()?\n    return Result.Ok(len(text) + len(lines))\n",
            &[],
            "",
        )
        .expect("program should run");

        assert_eq!(
            result.value.cli_text().as_deref(),
            Some("Result.Ok(value: 0)")
        );
    }

    #[test]
    fn unix_time_helpers_report_current_wall_clock_values() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    seconds = unix_seconds()?\n    millis = unix_millis()?\n    assert(seconds > 0, \"expected unix seconds to be positive\")\n    assert(millis >= seconds * 1000, \"expected unix millis to be at least seconds * 1000\")\n    assert(millis < (seconds + 2) * 1000, \"expected unix millis to stay close to unix seconds\")\n    return Result.Ok(1)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 1)"));
    }

    #[test]
    fn unix_time_helpers_reject_pre_epoch_and_overflow_clock_values() {
        let pre_epoch = super::unix_seconds_from_system_time(UNIX_EPOCH - Duration::from_secs(1))
            .expect_err("pre-epoch clocks should fail");
        assert!(pre_epoch.contains("before the Unix epoch"));

        let second_overflow =
            super::unix_seconds_from_duration_since_epoch(Duration::from_secs(i64::MAX as u64 + 1))
                .expect_err("out-of-range unix seconds should fail");
        assert!(second_overflow.contains("exceeds bootstrap int range"));

        let millis_overflow = super::unix_millis_from_duration_since_epoch(Duration::from_millis(
            i64::MAX as u64 + 1,
        ))
        .expect_err("out-of-range unix millis should fail");
        assert!(millis_overflow.contains("exceeds bootstrap int range"));
    }

    #[test]
    fn rejects_await_on_non_task() {
        let diagnostics =
            run_source("fn main():\n    return await 42\n").expect_err("await should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3009"]);
    }

    #[test]
    fn await_converts_result_task_failures_into_runtime_errors() {
        let value = run_source(
            "fn broken() -> Result[int, RuntimeError]:\n    return 1 / 0\nfn main() -> string:\n    task = go broken()\n    outcome = await task\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("result-returning task failure should stay inside Result");

        assert_eq!(
            value.cli_text().as_deref(),
            Some("RuntimeError.TaskFailed(message: GOF3068: `/` by zero is not allowed)")
        );
    }

    #[test]
    fn await_keeps_diagnostics_for_non_result_tasks() {
        let diagnostics = run_source(
            "fn broken() -> int:\n    return 1 / 0\nfn main() -> int:\n    task = go broken()\n    return await task\n",
        )
        .expect_err("plain task failure should still surface diagnostics");

        assert_eq!(diagnostics.codes(), vec!["GOF3068"]);
    }

    #[test]
    fn runtime_result_tasks_convert_panics_into_runtime_errors() {
        let task = TaskHandle::new(TaskBoundary::RuntimeResult);
        task.store_panic(TaskPanic {
            function_name: "worker".to_string(),
            source_path: PathBuf::from("worker.gof"),
            span: Span::new(1, 1, 1),
        });

        let value = task
            .await_value()
            .expect("runtime result task panic should be captured");

        assert_eq!(
            value.cli_text().as_deref(),
            Some("Result.Err(error: RuntimeError.TaskPanicked(task: worker))")
        );
    }

    #[test]
    fn direct_tasks_keep_panic_diagnostics() {
        let task = TaskHandle::new(TaskBoundary::Direct);
        task.store_panic(TaskPanic {
            function_name: "worker".to_string(),
            source_path: PathBuf::from("worker.gof"),
            span: Span::new(1, 1, 1),
        });

        let diagnostics = task
            .await_value()
            .expect_err("direct task panic should still be a diagnostic");

        assert_eq!(diagnostics.codes(), vec!["GOF3010"]);
    }

    #[test]
    fn await_result_wraps_successful_plain_tasks() {
        let value = run_source(
            "fn lucky() -> int:\n    return 21\nfn main() -> int:\n    task = go lucky()\n    outcome = await_result(task)\n    match outcome:\n        Result.Ok(value):\n            return value * 2\n        Result.Err(_):\n            return 0\n",
        )
        .expect("await_result should preserve successful task values");

        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn await_result_converts_plain_task_failures_into_runtime_errors() {
        let value = run_source(
            "fn broken() -> int:\n    return 1 / 0\nfn main() -> string:\n    task = go broken()\n    outcome = await_result(task)\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("await_result should capture plain task failures");

        assert_eq!(
            value.cli_text().as_deref(),
            Some("RuntimeError.TaskFailed(message: GOF3068: `/` by zero is not allowed)")
        );
    }

    #[test]
    fn await_result_converts_panics_into_runtime_errors() {
        let task = TaskHandle::new(TaskBoundary::Direct);
        task.store_panic(TaskPanic {
            function_name: "worker".to_string(),
            source_path: PathBuf::from("worker.gof"),
            span: Span::new(1, 1, 1),
        });

        let value = task.await_result_value(None);

        assert_eq!(
            value.cli_text().as_deref(),
            Some("Result.Err(error: RuntimeError.TaskPanicked(task: worker))")
        );
    }

    #[test]
    fn await_result_supports_optional_cancellation_tokens() {
        let value = run_source(
            "fn lucky() -> int:\n    return 7\nfn main() -> int:\n    token = cancel_token()\n    outcome = await_result(go lucky(), token)\n    match outcome:\n        Result.Ok(value):\n            return value\n        Result.Err(_):\n            return 0\n",
        )
        .expect("await_result with a live token should preserve successful joins");

        assert_eq!(value, Value::Int(7));
    }

    #[test]
    fn await_result_returns_cancelled_when_join_token_is_cancelled() {
        let value = run_source(
            "fn slow() -> int:\n    sleep(25)\n    return 7\nfn main() -> string:\n    task = go slow()\n    outcome = await_result(task, timeout_token(0))\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("await_result should surface cancellation as a runtime error result");

        assert_eq!(value.cli_text().as_deref(), Some("RuntimeError.Cancelled"));
    }

    #[test]
    fn await_result_prefers_ready_task_outcomes_over_cancelled_join_tokens() {
        let task = TaskHandle::new(TaskBoundary::Direct);
        task.store_value(Value::Int(9));
        let token = CancelTokenValue::new();
        token.cancel();

        let value = task.await_result_value(Some(&token));

        assert_eq!(value, result_ok(Value::Int(9)));
    }

    #[test]
    fn rejects_await_result_on_non_task() {
        let diagnostics =
            run_source("fn main() -> Result[int, RuntimeError]:\n    return await_result(42)\n")
                .expect_err("await_result should reject non-task operands");

        assert_eq!(diagnostics.codes(), vec!["GOF3009"]);
    }

    #[test]
    fn rejects_await_result_with_non_token_optional_argument() {
        let diagnostics = run_source(
            "fn lucky() -> int:\n    return 7\nfn main() -> Result[int, RuntimeError]:\n    task = go lucky()\n    return await_result(task, 1)\n",
        )
        .expect_err("await_result should reject non-token optional arguments");

        assert_eq!(diagnostics.codes(), vec!["GOF3082"]);
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
            "fn main() -> Result[int, RuntimeError]:\n    parsed = parse_int(trim(\" 41 \"))?\n    rendered = \"gof-\" + to_string(parsed + 1)\n    assert(rendered == \"gof-42\", \"expected converted text\")\n    return Result.Ok(parsed + len(rendered))\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 47)"));
    }

    #[test]
    fn evaluates_sequence_helper_builtins() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    values = [7, 1, 5, 3]\n    head = first(values)?\n    tail = last(values)?\n    middle = slice(values, 1, 3)?\n    reversed = reverse(values)\n    ordered = sort(values)\n    names = sort([\"warn\", \"critical\", \"ok\"])\n    smallest = min(values)?\n    loudest = max(names)?\n    assert(middle == [1, 5], \"expected middle slice\")\n    assert(reverse(middle) == [5, 1], \"expected reversed slice\")\n    assert(reversed == [3, 5, 1, 7], \"expected full reverse\")\n    assert(ordered == [1, 3, 5, 7], \"expected sorted ints\")\n    assert(names[0] == \"critical\", \"expected lexical string sort\")\n    assert(smallest == 1, \"expected min value\")\n    assert(loudest == \"warn\", \"expected max name\")\n    return Result.Ok(head + tail + len(middle) + len(names))\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 15)"));
    }

    #[test]
    fn prefers_user_functions_over_builtin_name_collisions() {
        let value = run_source(
            "fn first(values: list[int]) -> int:\n    return values[0] + 10\nfn main() -> int:\n    return first([7, 9])\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(17));
    }

    #[test]
    fn returns_empty_sequence_errors_as_runtime_values() {
        let value = run_source(
            "fn main() -> string:\n    outcome = first([])\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");
        assert_eq!(
            value,
            Value::String(
                "RuntimeError.EmptySequence(message: `first` requires a non-empty list)"
                    .to_string()
            )
        );
    }

    #[test]
    fn returns_extrema_errors_as_runtime_values() {
        let min_value = run_source(
            "fn main() -> string:\n    outcome = min([])\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");
        let max_value = run_source(
            "fn main() -> string:\n    outcome = max([])\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");

        assert_eq!(
            min_value,
            Value::String(
                "RuntimeError.EmptySequence(message: `min` requires a non-empty list)".to_string()
            )
        );
        assert_eq!(
            max_value,
            Value::String(
                "RuntimeError.EmptySequence(message: `max` requires a non-empty list)".to_string()
            )
        );
    }

    #[test]
    fn returns_slice_errors_as_runtime_values() {
        let value = run_source(
            "fn main() -> string:\n    outcome = slice([1, 2, 3], 2, 1)\n    match outcome:\n        Result.Ok(value):\n            return to_string(len(value))\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");
        assert_eq!(
            value,
            Value::String(
                "RuntimeError.Slice(message: `slice` start index cannot exceed end index)"
                    .to_string()
            )
        );
    }

    #[test]
    fn returns_parse_int_failures_as_runtime_errors() {
        let value = run_source(
            "fn main() -> string:\n    outcome = parse_int(\"oops\")\n    match outcome:\n        Result.Ok(value):\n            return to_string(value)\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");
        assert_eq!(
            value,
            Value::String(
                "RuntimeError.ParseInt(message: failed to parse int from `oops`: invalid digit found in string)"
                    .to_string()
            )
        );
    }

    #[test]
    fn evaluates_sleep_builtin() {
        let value = run_source("fn main() -> int:\n    sleep(0)\n    return 1\n")
            .expect("program should run");
        assert_eq!(value, Value::Int(1));
    }

    #[test]
    fn evaluates_timeout_token_and_cancel_after_builtins() {
        let value = run_source(
            "fn main() -> int:\n    expected: Result[int, RuntimeError] = Result.Err(RuntimeError.Cancelled)\n    ch: channel[int] = channel()\n    timed = recv(ch, timeout_token(0)) == expected\n    token: cancel_token = cancel_token()\n    cancel_after(token, 0)\n    scheduled = recv(ch, token) == expected\n    if timed and scheduled and is_cancelled(token):\n        return 42\n    return 0\n",
        )
        .expect("program should run");
        assert_eq!(value, Value::Int(42));
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
    fn evaluates_http_request_builtin_with_headers_and_structured_response() {
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

            let body = "accepted";
            let response = format!(
                "HTTP/1.1 202 Accepted\r\nContent-Type: text/plain; charset=utf-8\r\nX-Request-Id: req-42\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        });

        let value = run_source(&format!(
            "fn main() -> Result[int, RuntimeError]:\n    headers: dict[string] = {{\"Authorization\": \"Bearer test-token\", \"Content-Type\": \"text/plain\", \"X-Trace-Id\": \"trace-7\"}}\n    report = http_request(\"POST\", \"http://{address}/submit\", \"payload\", headers, 1500)?\n    status = json_int(json_get(report, \"status\")?)?\n    body = json_string(json_get(report, \"body\")?)?\n    method = json_string(json_get(report, \"method\")?)?\n    url = json_string(json_get(report, \"url\")?)?\n    response_headers = json_get(report, \"headers\")?\n    request_id_values = json_get(response_headers, \"x-request-id\")?\n    request_id = json_string(json_index(request_id_values, 0)?)?\n    content_type_values = json_get(response_headers, \"content-type\")?\n    response_content_type = json_string(json_index(content_type_values, 0)?)?\n    assert(status == 202, \"expected accepted response\")\n    assert(body == \"accepted\", \"expected response body to round-trip\")\n    assert(method == \"POST\", \"expected request method to be preserved\")\n    assert(url == \"http://{address}/submit\", \"expected response report URL\")\n    assert(request_id == \"req-42\", \"expected request id header\")\n    assert(starts_with(response_content_type, \"text/plain\"), \"expected content type header\")\n    return Result.Ok(status + len(body) + len(request_id))\n"
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
            requests
                .iter()
                .any(|request| request.contains("Authorization: Bearer test-token")),
            "expected authorization header, got {requests:?}"
        );
        assert!(
            requests
                .iter()
                .any(|request| request.contains("X-Trace-Id: trace-7")),
            "expected custom trace header, got {requests:?}"
        );
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 216)"));
    }

    #[test]
    fn http_request_preserves_non_success_status_reports() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener addr should exist");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 1024];
            let _ = stream
                .read(&mut buffer)
                .expect("request should be readable");

            let body = "retry later";
            let response = format!(
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        });

        let value = run_source(&format!(
            "fn main() -> Result[int, RuntimeError]:\n    report = http_request(\"GET\", \"http://{address}/health\")?\n    status = json_int(json_get(report, \"status\")?)?\n    body = json_string(json_get(report, \"body\")?)?\n    return Result.Ok(status + len(body))\n"
        ))
        .expect("program should run");

        server.join().expect("server thread should exit");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 514)"));
    }

    #[test]
    fn http_get_keeps_http_status_errors_for_legacy_body_only_calls() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener addr should exist");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 1024];
            let _ = stream
                .read(&mut buffer)
                .expect("request should be readable");

            let body = "retry later";
            let response = format!(
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        });

        let value = run_source(&format!(
            "fn main() -> Result[string, RuntimeError]:\n    return http_get(\"http://{address}/health\")\n"
        ))
        .expect("program should run");

        server.join().expect("server thread should exit");
        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.HttpStatus(code: 503"))
        );
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
    fn evaluates_line_oriented_file_io() {
        let temp = tempdir().expect("tempdir should exist");
        let output_path = temp.path().join("lines.txt");
        let output = output_path.to_string_lossy().replace('\\', "\\\\");

        let value = run_source(&format!(
            "fn main() -> Result[int, RuntimeError]:\n    path = \"{output}\"\n    write_lines(path, [\"alpha\", \"beta\", \"gamma\"])?\n    lines = read_lines(path)?\n    assert(lines[1] == \"beta\", \"expected middle line\")\n    return Result.Ok(len(lines) + len(join(lines, \"-\")))\n"
        ))
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 19)"));
        assert_eq!(
            std::fs::read_to_string(&output_path).expect("output file should exist"),
            "alpha\nbeta\ngamma"
        );
    }

    #[test]
    fn evaluates_run_process_builtin() {
        let executable = std::env::current_exe().expect("current executable should exist");
        let executable = executable.to_string_lossy().replace('\\', "\\\\");

        let value = run_source(&format!(
            "fn main() -> Result[int, RuntimeError]:\n    report = run_process(\"{executable}\", [\"--help\"])?\n    args = json_get(report, \"args\")?\n    status = json_int(json_get(report, \"status\")?)?\n    first = json_string(json_index(args, 0)?)?\n    stdout = json_string(json_get(report, \"stdout\")?)?\n    stderr = json_string(json_get(report, \"stderr\")?)?\n    assert(status == 0, \"expected successful process exit status\")\n    assert(len(stdout) > 0 or len(stderr) > 0, \"expected captured process output\")\n    return Result.Ok(status + json_len(args)? + len(first))\n"
        ))
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 7)"));
    }

    #[test]
    fn run_process_returns_runtime_errors_for_missing_programs() {
        let value = run_source(
            "fn main() -> string:\n    outcome = run_process(\"definitely-not-a-real-gof-command\", [\"--help\"])\n    match outcome:\n        Result.Ok(_):\n            return \"unexpected\"\n        Result.Err(error):\n            return to_string(error)\n",
        )
        .expect("program should run");

        let rendered = value
            .cli_text()
            .expect("missing-process result should render as text");
        assert!(rendered.starts_with("RuntimeError.Io(message:"));
        assert!(rendered.contains("failed to run `definitely-not-a-real-gof-command`"));
    }

    #[test]
    fn evaluates_csv_builtins() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    text = csv_stringify([[\"name\", \"count\"], [\"alpha\", \"2\"], [\"beta\", \"5\"]])?\n    rows = csv_parse(text)?\n    assert(rows[1][0] == \"alpha\", \"expected first row\")\n    return Result.Ok(len(rows) + parse_int(rows[2][1])?)\n",
        )
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 8)"));
    }

    #[test]
    fn evaluates_toml_parse_builtin() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    config = toml_parse(\"name = \\\"alpha\\\"\\nport = 7\\n[limits]\\nworkers = 5\")?\n    limits = json_get(config, \"limits\")?\n    workers = json_int(json_get(limits, \"workers\")?)?\n    name = json_string(json_get(config, \"name\")?)?\n    port = json_int(json_get(config, \"port\")?)?\n    return Result.Ok(len(name) + workers + port)\n",
        )
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 17)"));
    }

    #[test]
    fn evaluates_yaml_parse_builtin() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    config = yaml_parse(\"service: alpha\\nport: 7\\nlimits:\\n  workers: 5\")?\n    limits = json_get(config, \"limits\")?\n    workers = json_int(json_get(limits, \"workers\")?)?\n    name = json_string(json_get(config, \"service\")?)?\n    port = json_int(json_get(config, \"port\")?)?\n    return Result.Ok(len(name) + workers + port)\n",
        )
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 17)"));
    }

    #[test]
    fn evaluates_template_render_with_dict_and_json_contexts() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    context: dict = {\"name\": \"alpha\", \"workers\": 5}\n    left = template_render(\"service {{ name }} has {{workers}} workers\", context)?\n    config = toml_parse(\"name = \\\"beta\\\"\\nport = 7\")?\n    right = template_render(\"{{name}} listens on {{port}}\", config)?\n    assert(left == \"service alpha has 5 workers\", \"expected rendered dict template\")\n    assert(right == \"beta listens on 7\", \"expected rendered json template\")\n    return Result.Ok(len(left) + len(right))\n",
        )
        .expect("program should run");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 44)"));
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
    fn evaluates_explicit_channel_capacity_baseline() {
        let value = run_source(
            "fn echo(ch: channel[int]) -> Result[int, RuntimeError]:\n    return recv(ch)\nfn main() -> Result[int, RuntimeError]:\n    buffered: channel[int] = channel(2)\n    send(buffered, 3)?\n    send(buffered, 4)?\n    rendezvous: channel[int] = channel(0)\n    task = go echo(rendezvous)\n    send(rendezvous, 5)?\n    echoed = await task\n    match echoed:\n        Result.Ok(value):\n            return Result.Ok(recv(buffered)? + recv(buffered)? + value)\n        Result.Err(error):\n            return Result.Err(error)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 12)"));
    }

    #[test]
    fn rendezvous_channels_wait_for_receivers() {
        let channel = Arc::new(ChannelHandle::new(Some(0)));
        let sender_channel = Arc::clone(&channel);

        let sender = std::thread::spawn(move || sender_channel.send(Value::Int(7), None));

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !sender.is_finished(),
            "rendezvous send should block until a receiver is waiting"
        );

        let received = channel.recv(None);
        assert_eq!(
            sender.join().expect("sender thread should join"),
            ChannelReceiveState::Value(Value::Unit)
        );
        assert_eq!(received, ChannelReceiveState::Value(Value::Int(7)));
    }

    #[test]
    fn bounded_channels_block_senders_until_buffer_space_frees_up() {
        let channel = Arc::new(ChannelHandle::new(Some(1)));
        assert_eq!(
            channel.send(Value::Int(1), None),
            ChannelReceiveState::Value(Value::Unit)
        );

        let sender_channel = Arc::clone(&channel);
        let sender = std::thread::spawn(move || sender_channel.send(Value::Int(2), None));

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !sender.is_finished(),
            "bounded send should wait while the buffer is full"
        );

        assert_eq!(
            channel.recv(None),
            ChannelReceiveState::Value(Value::Int(1))
        );
        assert_eq!(
            sender.join().expect("sender thread should join"),
            ChannelReceiveState::Value(Value::Unit)
        );
        assert_eq!(
            channel.recv(None),
            ChannelReceiveState::Value(Value::Int(2))
        );
    }

    #[test]
    fn rendezvous_channels_expose_pending_sends_to_try_recv() {
        let channel = Arc::new(ChannelHandle::new(Some(0)));
        let sender_channel = Arc::clone(&channel);
        let sender = std::thread::spawn(move || sender_channel.send(Value::Int(9), None));

        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            channel.try_recv(None),
            Some(ChannelReceiveState::Value(Value::Int(9)))
        );
        assert_eq!(
            sender.join().expect("sender thread should join"),
            ChannelReceiveState::Value(Value::Unit)
        );
    }

    #[test]
    fn evaluates_select_default_arm_when_no_receive_is_ready() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel()\n    select:\n        received = recv(ch):\n            return received\n        default:\n            return Result.Ok(7)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 7)"));
    }

    #[test]
    fn prefers_ready_receive_over_select_default_arm() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel()\n    send(ch, 9)?\n    select:\n        received = recv(ch):\n            return Result.Ok(received? + 1)\n        default:\n            return Result.Ok(0)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 10)"));
    }

    #[test]
    fn evaluates_select_send_arm_when_channel_is_ready() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel(1)\n    select:\n        sent = send(ch, 7):\n            sent?\n            return Result.Ok(recv(ch)? + 1)\n        default:\n            return Result.Ok(0)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 8)"));
    }

    #[test]
    fn select_send_arm_falls_back_to_default_when_send_would_block() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel(1)\n    send(ch, 1)?\n    select:\n        sent = send(ch, 2):\n            sent?\n            return Result.Ok(99)\n        default:\n            return Result.Ok(recv(ch)?)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 1)"));
    }

    #[test]
    fn rotates_select_arm_priority_when_multiple_receives_are_ready() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    mut left_hits = 0\n    mut right_hits = 0\n    mut i = 0\n    while i < 16:\n        left: channel = channel()\n        right: channel = channel()\n        send(left, 1)?\n        send(right, 1)?\n        select:\n            received = recv(left):\n                left_hits = left_hits + received?\n            received = recv(right):\n                right_hits = right_hits + received?\n        i = i + 1\n    assert(left_hits == 8, \"expected round-robin select polling to choose the left arm exactly eight times\")\n    assert(right_hits == 8, \"expected round-robin select polling to choose the right arm exactly eight times\")\n    return Result.Ok(left_hits + right_hits)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 16)"));
    }

    #[test]
    fn rejects_duplicate_select_default_arms() {
        let diagnostics = run_source(
            "fn main() -> int:\n    select:\n        default:\n            return 1\n        default:\n            return 2\n",
        )
        .expect_err("duplicate default arms should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3095"]);
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
    fn evaluates_explicit_comparison_semantics() {
        let value = run_source(
            "struct Snapshot:\n    count: int\n    label: string\n\nenum Stage:\n    Draft\n    Published(version: int)\n\nfn main() -> Result[int, RuntimeError]:\n    left = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    right = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    snapshot_a: Snapshot = Snapshot(2, \"beta\")\n    snapshot_b: Snapshot = Snapshot(2, \"beta\")\n    stage_a: Stage = Stage.Published(3)\n    stage_b: Stage = Stage.Published(3)\n    ok_a: Result[int, RuntimeError] = Result.Ok(7)\n    ok_b: Result[int, RuntimeError] = Result.Ok(7)\n    assert(\"alpha\" < \"beta\", \"expected lexicographic string ordering\")\n    assert(left == right, \"expected structural json equality\")\n    assert(snapshot_a == snapshot_b, \"expected structural struct equality\")\n    assert(stage_a == stage_b, \"expected payload enum equality\")\n    assert(ok_a == ok_b, \"expected result equality\")\n    assert(sleep(0) == sleep(0), \"expected unit equality\")\n    assert([1, 2] == [1, 2], \"expected list equality\")\n    assert({\"ok\": 2} == {\"ok\": 2}, \"expected dict equality\")\n    return Result.Ok(42)\n",
        )
        .expect("program should run");
        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 42)"));
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
    fn write_lines_rejects_non_string_lists_at_runtime() {
        let temp = tempdir().expect("tempdir should exist");
        let output_path = temp.path().join("lines.txt");
        let output = output_path.to_string_lossy().replace('\\', "\\\\");

        let diagnostics = run_source(&format!(
            "fn persist(path: string, lines: list) -> Result[unit, RuntimeError]:\n    return write_lines(path, lines)\nfn main() -> Result[unit, RuntimeError]:\n    return persist(\"{output}\", [1, 2])\n"
        ))
        .expect_err("write_lines should reject non-string line lists at runtime");

        assert_eq!(diagnostics.codes(), vec!["GOF3043"]);
    }

    #[test]
    fn run_process_rejects_non_string_argument_lists_at_runtime() {
        let diagnostics = run_source(
            "fn invoke(program: string, args: list) -> Result[json, RuntimeError]:\n    return run_process(program, args)\nfn main() -> Result[json, RuntimeError]:\n    return invoke(\"gof\", [1, 2])\n",
        )
        .expect_err("run_process should reject non-string arg lists at runtime");

        assert_eq!(diagnostics.codes(), vec!["GOF3100"]);
    }

    #[test]
    fn csv_stringify_rejects_non_string_rows_at_runtime() {
        let diagnostics = run_source(
            "fn encode(rows: list) -> Result[string, RuntimeError]:\n    return csv_stringify(rows)\nfn main() -> Result[string, RuntimeError]:\n    return encode([[1], [2]])\n",
        )
        .expect_err("csv_stringify should reject non-string rows at runtime");

        assert_eq!(diagnostics.codes(), vec!["GOF3098"]);
    }

    #[test]
    fn template_render_rejects_invalid_context_operands() {
        let value = run_source(
            "fn render(values: list) -> Result[string, RuntimeError]:\n    return template_render(\"hello {{name}}\", values)\nfn main() -> Result[string, RuntimeError]:\n    return render([\"gof\"])\n",
        )
        .expect("template_render should return a runtime error value");

        assert!(value.cli_text().as_deref().is_some_and(|text| {
            text.contains("template_render expects `dict[...]` or `json`, got `list`")
        }));
    }

    #[test]
    fn csv_parse_returns_runtime_errors_for_malformed_csv() {
        let value = run_source(
            "fn main() -> Result[list[list[string]], RuntimeError]:\n    return csv_parse(\"name,count\\nalpha,2\\nbeta\")\n",
        )
        .expect("csv parse should return a runtime error value");

        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.Csv(message:"))
        );
    }

    #[test]
    fn toml_parse_returns_runtime_errors_for_unsupported_scalars() {
        let value = run_source(
            "fn main() -> Result[json, RuntimeError]:\n    return toml_parse(\"ratio = 1.5\")\n",
        )
        .expect("toml parse should return a runtime error value");

        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.Toml(message:"))
        );
    }

    #[test]
    fn yaml_parse_returns_runtime_errors_for_unsupported_yaml_shapes() {
        let value = run_source(
            "fn main() -> Result[json, RuntimeError]:\n    return yaml_parse(\"? [1, 2]\\n: bad\")\n",
        )
        .expect("yaml parse should return a runtime error value");

        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.Yaml(message:"))
        );
    }

    #[test]
    fn evaluates_base64_builtins() {
        let value = run_source(
            "fn main() -> Result[int, RuntimeError]:\n    encoded = base64_encode(\"gof!\")\n    decoded = base64_decode(encoded)?\n    return Result.Ok(len(encoded) + len(decoded))\n",
        )
        .expect("base64 helpers should succeed");

        assert_eq!(value.cli_text().as_deref(), Some("Result.Ok(value: 12)"));
    }

    #[test]
    fn base64_decode_returns_runtime_errors_for_invalid_input() {
        let value = run_source(
            "fn main() -> Result[string, RuntimeError]:\n    return base64_decode(\"%%%\")\n",
        )
        .expect("base64 decode should return a runtime error value");

        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.Base64(message:"))
        );
    }

    #[test]
    fn template_render_returns_runtime_errors_for_missing_keys() {
        let value = run_source(
            "fn main() -> Result[string, RuntimeError]:\n    return template_render(\"hello {{name}}\", {\"title\": \"gof\"})\n",
        )
        .expect("template_render should return a runtime error value");

        assert!(
            value
                .cli_text()
                .as_deref()
                .is_some_and(|text| text.contains("RuntimeError.Template(message:"))
        );
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
        let diagnostics =
            run_source("fn main() -> Result[int, RuntimeError]:\n    return parse_int(1)\n")
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
    fn rejects_invalid_timeout_token_duration() {
        let diagnostics =
            run_source("fn main() -> cancel_token:\n    return timeout_token(\"soon\")\n")
                .expect_err("timeout_token operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3093"]);
    }

    #[test]
    fn rejects_negative_timeout_cancellation_durations() {
        let diagnostics = run_source("fn main() -> unit:\n    cancel_after(cancel_token(), -1)\n")
            .expect_err("negative cancel_after duration should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3094"]);
    }

    #[test]
    fn rejects_dynamic_negative_channel_capacity() {
        let diagnostics = run_source(
            "fn main() -> channel[int]:\n    mut zero = 0\n    return channel(zero - 1)\n",
        )
        .expect_err("negative channel capacity should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3097"]);
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
    fn rejects_invalid_first_operand() {
        let diagnostics =
            run_source("fn main() -> Result[int, RuntimeError]:\n    return first(1)\n")
                .expect_err("first operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_sort_operand() {
        let diagnostics = run_source("fn main() -> list[bool]:\n    return sort([true, false])\n")
            .expect_err("sort operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_min_operand() {
        let diagnostics =
            run_source("fn main() -> Result[int, RuntimeError]:\n    return min([true, false])\n")
                .expect_err("min operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_max_operand() {
        let diagnostics =
            run_source("fn main() -> Result[int, RuntimeError]:\n    return max(1)\n")
                .expect_err("max operand should fail");
        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
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
