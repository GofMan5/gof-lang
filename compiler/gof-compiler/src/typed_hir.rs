use crate::ast::{BinaryOp, UnaryOp};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::hir::{
    HirEnum, HirExpr, HirFunction, HirMatchArm, HirMatchPattern, HirModule, HirSelectArm,
    HirSelectArmKind, HirStmt, HirStruct, HirTypeRef,
};
use crate::source::Span;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Type {
    Int,
    String,
    Bool,
    Json,
    CancelToken,
    Struct(String),
    Enum(String),
    List(Box<Type>),
    Dict(Box<Type>),
    Channel(Box<Type>),
    Task(Box<Type>),
    Result(Box<Type>, Box<Type>),
    Unknown,
    Unit,
}

impl Type {
    fn task(inner: Type) -> Self {
        Self::Task(Box::new(inner))
    }

    fn list(inner: Type) -> Self {
        Self::List(Box::new(inner))
    }

    fn dict(inner: Type) -> Self {
        Self::Dict(Box::new(inner))
    }

    fn channel(inner: Type) -> Self {
        Self::Channel(Box::new(inner))
    }

    fn result(ok: Type, err: Type) -> Self {
        Self::Result(Box::new(ok), Box::new(err))
    }

    fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    fn display_name(&self) -> String {
        match self {
            Self::Int => "int".to_string(),
            Self::String => "string".to_string(),
            Self::Bool => "bool".to_string(),
            Self::Json => "json".to_string(),
            Self::CancelToken => "cancel_token".to_string(),
            Self::Struct(name) => name.clone(),
            Self::Enum(name) => name.clone(),
            Self::List(inner) => format!("list[{}]", inner.display_name()),
            Self::Dict(inner) => format!("dict[{}]", inner.display_name()),
            Self::Channel(inner) => format!("channel[{}]", inner.display_name()),
            Self::Task(inner) => format!("task[{}]", inner.display_name()),
            Self::Result(ok, err) => {
                format!("Result[{}, {}]", ok.display_name(), err.display_name())
            }
            Self::Unknown => "unknown".to_string(),
            Self::Unit => "unit".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedModule {
    pub structs: Vec<TypedStruct>,
    pub enums: Vec<TypedEnum>,
    pub functions: Vec<TypedFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedFunction {
    pub id: usize,
    pub symbol_name: String,
    pub receiver_type: Option<Type>,
    pub name: String,
    pub params: Vec<TypedParam>,
    pub return_type: Type,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedStruct {
    pub name: String,
    pub fields: Vec<TypedStructField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedEnum {
    pub name: String,
    pub variants: Vec<TypedEnumVariant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedStructField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedEnumVariant {
    pub name: String,
    pub fields: Vec<TypedEnumVariantField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedEnumVariantField {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedParam {
    pub name: String,
    pub ty: Type,
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
    Break,
    Continue,
    If {
        condition: TypedExpr,
        then_body: Vec<TypedStmt>,
        else_body: Vec<TypedStmt>,
    },
    While {
        condition: TypedExpr,
        body: Vec<TypedStmt>,
    },
    For {
        binding: String,
        iterable: TypedExpr,
        body: Vec<TypedStmt>,
    },
    Match {
        value: TypedExpr,
        arms: Vec<TypedMatchArm>,
    },
    Select {
        arms: Vec<TypedSelectArm>,
    },
    Expr(TypedExpr),
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedMatchArm {
    pub pattern: TypedMatchPattern,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TypedMatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        bindings: Vec<TypedMatchBinding>,
        span: Span,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedMatchBinding {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedSelectArm {
    pub binding: Option<String>,
    pub kind: TypedSelectArmKind,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub enum TypedSelectArmKind {
    Recv { operation: TypedExpr },
    Send { operation: TypedExpr },
    Default,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedDictEntry {
    pub key: TypedExpr,
    pub value: TypedExpr,
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
    List {
        items: Vec<TypedExpr>,
    },
    Dict {
        entries: Vec<TypedDictEntry>,
    },
    Call {
        callee: String,
        args: Vec<TypedExpr>,
    },
    MethodCall {
        target: Box<TypedExpr>,
        method: String,
        symbol_name: String,
        args: Vec<TypedExpr>,
    },
    StructInit {
        name: String,
        args: Vec<TypedExpr>,
    },
    EnumVariant {
        enum_name: String,
        variant: String,
        args: Vec<TypedExpr>,
    },
    Field {
        target: Box<TypedExpr>,
        field: String,
    },
    Index {
        target: Box<TypedExpr>,
        index: Box<TypedExpr>,
    },
    Spawn {
        callee: String,
        args: Vec<TypedExpr>,
    },
    Await {
        value: Box<TypedExpr>,
    },
    Propagate {
        value: Box<TypedExpr>,
    },
    Unary {
        op: UnaryOp,
        value: Box<TypedExpr>,
    },
    Binary {
        lhs: Box<TypedExpr>,
        op: BinaryOp,
        rhs: Box<TypedExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
    symbol_name: String,
    receiver_type: Option<String>,
    arity: usize,
    param_types: Vec<Type>,
    declared_return_type: Option<Type>,
    return_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructSignature {
    fields: Vec<StructFieldSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumSignature {
    variants: Vec<EnumVariantSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructFieldSignature {
    name: String,
    ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumVariantSignature {
    name: String,
    fields: Vec<EnumVariantFieldSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumVariantFieldSignature {
    name: String,
    ty: Type,
}

#[derive(Debug, Clone)]
struct LocalBinding {
    mutable: bool,
    ty: Type,
}

#[derive(Debug, Clone)]
enum MatchTargetKind {
    Enum(String),
    Result { ok: Type, err: Type },
}

impl MatchTargetKind {
    fn display_name(&self) -> &str {
        match self {
            Self::Enum(name) => name,
            Self::Result { .. } => "Result",
        }
    }
}

fn builtin_enum_signatures() -> HashMap<String, EnumSignature> {
    HashMap::from([(
        "RuntimeError".to_string(),
        EnumSignature {
            variants: vec![
                EnumVariantSignature {
                    name: "EnvMissing".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "name".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "Io".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "ChannelClosed".to_string(),
                    fields: Vec::new(),
                },
                EnumVariantSignature {
                    name: "Cancelled".to_string(),
                    fields: Vec::new(),
                },
                EnumVariantSignature {
                    name: "TaskFailed".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "TaskPanicked".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "task".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "ParseInt".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "EmptySequence".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "Slice".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "Json".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "HttpRequest".to_string(),
                    fields: vec![EnumVariantFieldSignature {
                        name: "message".to_string(),
                        ty: Type::String,
                    }],
                },
                EnumVariantSignature {
                    name: "HttpStatus".to_string(),
                    fields: vec![
                        EnumVariantFieldSignature {
                            name: "code".to_string(),
                            ty: Type::Int,
                        },
                        EnumVariantFieldSignature {
                            name: "body".to_string(),
                            ty: Type::String,
                        },
                    ],
                },
            ],
        },
    )])
}

#[derive(Debug, Clone)]
struct ScopeStack {
    scopes: Vec<HashMap<String, LocalBinding>>,
}

impl ScopeStack {
    fn new(params: &[TypedParam]) -> Self {
        let root = params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    LocalBinding {
                        mutable: false,
                        ty: param.ty.clone(),
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
            .find_map(|scope| scope.get(name).cloned())
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

#[derive(Debug, Clone)]
struct ReturnAccumulator {
    ty: Option<Type>,
    span: Option<Span>,
    incompatible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    BuiltinLen,
    BuiltinPrint,
    BuiltinAppend,
    BuiltinContains,
    BuiltinFirst,
    BuiltinLast,
    BuiltinSlice,
    BuiltinReverse,
    BuiltinSort,
    BuiltinMin,
    BuiltinMax,
    BuiltinTrim,
    BuiltinSplit,
    BuiltinJoin,
    BuiltinStartsWith,
    BuiltinEndsWith,
    BuiltinParseInt,
    BuiltinToString,
    BuiltinRange,
    BuiltinSleep,
    BuiltinAssert,
    BuiltinArgv,
    BuiltinEnv,
    BuiltinCwd,
    BuiltinExists,
    BuiltinReadDir,
    BuiltinMkdir,
    BuiltinRemoveFile,
    BuiltinPathJoin,
    BuiltinPathDir,
    BuiltinPathBase,
    BuiltinPathExt,
    BuiltinReadFile,
    BuiltinWriteFile,
    BuiltinDict,
    BuiltinInsert,
    BuiltinKeys,
    BuiltinValues,
    BuiltinChannel,
    BuiltinClose,
    BuiltinSend,
    BuiltinRecv,
    BuiltinCancelToken,
    BuiltinCancel,
    BuiltinIsCancelled,
    BuiltinTimeoutToken,
    BuiltinCancelAfter,
    BuiltinJsonParse,
    BuiltinJsonStringify,
    BuiltinJsonGet,
    BuiltinJsonIndex,
    BuiltinJsonLen,
    BuiltinJsonString,
    BuiltinJsonInt,
    BuiltinHttpGet,
    BuiltinHttpPost,
    Function,
    Struct,
    Enum,
    Unknown,
}

pub fn lower(module: &HirModule) -> Result<TypedModule, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let builtin_enums = builtin_enum_signatures();
    let known_structs = module
        .structs
        .iter()
        .map(|decl| decl.name.clone())
        .collect::<HashSet<_>>();
    let mut known_enums = module
        .enums
        .iter()
        .map(|decl| decl.name.clone())
        .collect::<HashSet<_>>();
    known_enums.extend(builtin_enums.keys().cloned());
    let struct_signatures = module
        .structs
        .iter()
        .map(|decl| {
            (
                decl.name.clone(),
                lower_struct_signature(decl, &known_structs, &known_enums, &mut diagnostics),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut enum_signatures = module
        .enums
        .iter()
        .map(|decl| {
            (
                decl.name.clone(),
                lower_enum_signature(decl, &known_structs, &known_enums, &mut diagnostics),
            )
        })
        .collect::<HashMap<_, _>>();
    enum_signatures.extend(builtin_enums);
    let mut signatures = HashMap::new();
    let mut method_signatures = HashMap::new();
    for function in &module.functions {
        let declared_return_type = resolve_type_annotation(
            function.return_type.as_ref(),
            &known_structs,
            &known_enums,
            &function.source_path,
            &mut diagnostics,
        );
        let declared_return_type = if function.return_type.is_some() {
            Some(declared_return_type)
        } else {
            None
        };
        let param_types = function
            .params
            .iter()
            .map(|param| {
                resolve_type_annotation(
                    param.ty.as_ref(),
                    &known_structs,
                    &known_enums,
                    &function.source_path,
                    &mut diagnostics,
                )
            })
            .collect::<Vec<_>>();
        let symbol_name = function_symbol_from_hir(function);

        if let Some(receiver_type) = &function.receiver_type {
            validate_method_contract(
                function,
                receiver_type,
                &param_types,
                &known_structs,
                &mut diagnostics,
            );
            method_signatures.insert(
                (receiver_type.name.clone(), function.name.clone()),
                FunctionSignature {
                    symbol_name,
                    receiver_type: Some(receiver_type.name.clone()),
                    arity: function.params.len(),
                    param_types,
                    declared_return_type: declared_return_type.clone(),
                    return_type: declared_return_type.unwrap_or(Type::Unknown),
                },
            );
        } else {
            signatures.insert(
                function.name.clone(),
                FunctionSignature {
                    symbol_name,
                    receiver_type: None,
                    arity: function.params.len(),
                    param_types,
                    declared_return_type: declared_return_type.clone(),
                    return_type: declared_return_type.unwrap_or(Type::Unknown),
                },
            );
        }
    }

    for _ in 0..=module.functions.len() {
        let typed_functions = module
            .functions
            .iter()
            .map(|function| {
                lower_function(
                    function,
                    &signatures,
                    &method_signatures,
                    &known_structs,
                    &known_enums,
                    &struct_signatures,
                    &enum_signatures,
                    &mut Diagnostics::default(),
                )
            })
            .collect::<Vec<_>>();

        let mut changed = false;
        for function in &typed_functions {
            let signature = if let Some(Type::Struct(receiver_type)) = &function.receiver_type {
                method_signatures
                    .get_mut(&(receiver_type.clone(), function.name.clone()))
                    .expect("all lowered methods must have a matching signature entry")
            } else {
                signatures
                    .get_mut(&function.name)
                    .expect("all lowered functions must have a matching signature entry")
            };
            if signature.return_type != function.return_type {
                signature.return_type = function.return_type.clone();
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    let structs = module
        .structs
        .iter()
        .map(|decl| lower_struct(decl, &struct_signatures))
        .collect::<Vec<_>>();
    let enums = module
        .enums
        .iter()
        .map(|decl| lower_enum(decl, &enum_signatures))
        .collect::<Vec<_>>();
    let functions = module
        .functions
        .iter()
        .map(|function| {
            lower_function(
                function,
                &signatures,
                &method_signatures,
                &known_structs,
                &known_enums,
                &struct_signatures,
                &enum_signatures,
                &mut diagnostics,
            )
        })
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        Ok(TypedModule {
            structs,
            enums,
            functions,
        })
    } else {
        Err(diagnostics)
    }
}

fn function_symbol_from_hir(function: &HirFunction) -> String {
    match &function.receiver_type {
        Some(receiver_type) => format!("{}.{}", receiver_type.name, function.name),
        None => function.name.clone(),
    }
}

fn validate_method_contract(
    function: &HirFunction,
    receiver_type: &HirTypeRef,
    param_types: &[Type],
    known_structs: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    if !known_structs.contains(&receiver_type.name) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3035",
                format!(
                    "method `{}` must target a known struct receiver",
                    function_symbol_from_hir(function)
                ),
                format!(
                    "`{}` is not a declared struct in this module graph",
                    receiver_type.name
                ),
                receiver_type.span,
            )
            .with_fix_it("declare the struct first or move this method onto a known struct type")
            .with_source_path(function.source_path.clone()),
        );
        return;
    }

    let Some(first_param) = function.params.first() else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3035",
                format!(
                    "method `{}` must declare an explicit receiver parameter",
                    function_symbol_from_hir(function)
                ),
                "the first parameter must carry the same struct type as the declared receiver",
                receiver_type.span,
            )
            .with_fix_it(format!(
                "add a first parameter like `self: {}` to this method",
                receiver_type.name
            ))
            .with_source_path(function.source_path.clone()),
        );
        return;
    };

    let actual_receiver_type = param_types.first().cloned().unwrap_or(Type::Unknown);
    if actual_receiver_type != Type::Struct(receiver_type.name.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3035",
                format!(
                    "method `{}` has an incompatible receiver parameter",
                    function_symbol_from_hir(function)
                ),
                format!(
                    "the first parameter resolves to `{}`, but the method receiver is `{}`",
                    actual_receiver_type.display_name(),
                    receiver_type.name
                ),
                first_param.span,
            )
            .with_fix_it(format!(
                "annotate the first parameter as `{}`",
                receiver_type.name
            ))
            .with_source_path(function.source_path.clone()),
        );
    }
}

fn lower_function(
    function: &HirFunction,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
) -> TypedFunction {
    let signature = if let Some(receiver_type) = &function.receiver_type {
        method_signatures
            .get(&(receiver_type.name.clone(), function.name.clone()))
            .expect("each method should have a signature during lowering")
    } else {
        signatures
            .get(&function.name)
            .expect("each function should have a signature during lowering")
    };
    let params = function
        .params
        .iter()
        .zip(signature.param_types.iter())
        .map(|(param, ty)| TypedParam {
            name: param.name.clone(),
            ty: ty.clone(),
        })
        .collect::<Vec<_>>();
    let mut scopes = ScopeStack::new(&params);
    let body = lower_block(
        &function.body,
        &mut scopes,
        signatures,
        method_signatures,
        known_structs,
        known_enums,
        struct_signatures,
        enum_signatures,
        diagnostics,
        0,
        false,
        &signature.return_type,
        &function.source_path,
    );
    let inferred_return_type =
        infer_return_type(&function.name, &body, diagnostics, &function.source_path);
    let return_type = match &signature.declared_return_type {
        Some(declared_return_type) => {
            ensure_type_compatibility(
                declared_return_type,
                &inferred_return_type,
                function
                    .return_type
                    .as_ref()
                    .expect("declared return type should have an annotation span")
                    .span,
                diagnostics,
                format!(
                    "declared return type for `{}` is incompatible with its body",
                    function.name
                ),
                format!(
                    "the function is declared as `{}`, but its return paths resolve to `{}`",
                    declared_return_type.display_name(),
                    inferred_return_type.display_name()
                ),
                &function.source_path,
            );
            declared_return_type.clone()
        }
        None => inferred_return_type,
    };

    TypedFunction {
        id: function.id,
        symbol_name: signature.symbol_name.clone(),
        receiver_type: signature
            .receiver_type
            .as_ref()
            .map(|receiver_type| Type::Struct(receiver_type.clone())),
        name: function.name.clone(),
        params,
        return_type,
        body,
    }
}

fn lower_struct_signature(
    decl: &HirStruct,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) -> StructSignature {
    let mut seen_fields = HashSet::new();
    StructSignature {
        fields: decl
            .fields
            .iter()
            .map(|field| {
                if !seen_fields.insert(field.name.clone()) {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3023",
                            format!("duplicate field `{}` on `{}`", field.name, decl.name),
                            "each struct field name must be unique within its declaration",
                            field.span,
                        )
                        .with_fix_it("rename or remove the duplicate field")
                        .with_source_path(decl.source_path.clone()),
                    );
                }

                StructFieldSignature {
                    name: field.name.clone(),
                    ty: resolve_type_annotation(
                        Some(&field.ty),
                        known_structs,
                        known_enums,
                        &decl.source_path,
                        diagnostics,
                    ),
                }
            })
            .collect(),
    }
}

fn lower_enum_signature(
    decl: &HirEnum,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) -> EnumSignature {
    let mut seen_variants = HashSet::new();
    EnumSignature {
        variants: decl
            .variants
            .iter()
            .map(|variant| {
                if !seen_variants.insert(variant.name.clone()) {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3027",
                            format!("duplicate variant `{}` on `{}`", variant.name, decl.name),
                            "each enum variant name must be unique within its declaration",
                            variant.span,
                        )
                        .with_fix_it("rename or remove the duplicate variant")
                        .with_source_path(decl.source_path.clone()),
                    );
                }

                EnumVariantSignature {
                    name: variant.name.clone(),
                    fields: variant
                        .fields
                        .iter()
                        .map(|field| EnumVariantFieldSignature {
                            name: field.name.clone(),
                            ty: resolve_type_annotation(
                                Some(&field.ty),
                                known_structs,
                                known_enums,
                                &decl.source_path,
                                diagnostics,
                            ),
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

fn lower_struct(
    decl: &HirStruct,
    struct_signatures: &HashMap<String, StructSignature>,
) -> TypedStruct {
    let signature = struct_signatures
        .get(&decl.name)
        .expect("typed struct lowering requires a matching struct signature");
    TypedStruct {
        name: decl.name.clone(),
        fields: signature
            .fields
            .iter()
            .map(|field| TypedStructField {
                name: field.name.clone(),
                ty: field.ty.clone(),
            })
            .collect(),
    }
}

fn lower_enum(decl: &HirEnum, enum_signatures: &HashMap<String, EnumSignature>) -> TypedEnum {
    let signature = enum_signatures
        .get(&decl.name)
        .expect("typed enum lowering requires a matching enum signature");
    TypedEnum {
        name: decl.name.clone(),
        variants: signature
            .variants
            .iter()
            .map(|variant| TypedEnumVariant {
                name: variant.name.clone(),
                fields: variant
                    .fields
                    .iter()
                    .map(|field| TypedEnumVariantField {
                        name: field.name.clone(),
                        ty: field.ty.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn lower_block(
    stmts: &[HirStmt],
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    loop_depth: usize,
    nested_scope: bool,
    function_return_type: &Type,
    source_path: &Path,
) -> Vec<TypedStmt> {
    if nested_scope {
        scopes.push();
    }

    let body = stmts
        .iter()
        .map(|stmt| {
            lower_stmt(
                stmt,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                function_return_type,
                source_path,
            )
        })
        .collect::<Vec<_>>();

    if nested_scope {
        scopes.pop();
    }

    body
}

fn lower_stmt(
    stmt: &HirStmt,
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    loop_depth: usize,
    function_return_type: &Type,
    source_path: &Path,
) -> TypedStmt {
    match stmt {
        HirStmt::Return(expr, _) => TypedStmt::Return(lower_expr(
            expr,
            scopes,
            signatures,
            method_signatures,
            known_structs,
            known_enums,
            struct_signatures,
            enum_signatures,
            diagnostics,
            function_return_type,
            source_path,
        )),
        HirStmt::Bind {
            name,
            mutable,
            ty,
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
                    .with_fix_it("rename this binding or reuse the existing one")
                    .with_source_path(source_path.to_path_buf()),
                );
            }

            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let declared_type = resolve_type_annotation(
                ty.as_ref(),
                known_structs,
                known_enums,
                source_path,
                diagnostics,
            );
            if ty.is_some() {
                ensure_type_compatibility(
                    &declared_type,
                    &value.ty,
                    *span,
                    diagnostics,
                    format!("binding `{name}` has incompatible annotated type"),
                    format!(
                        "the binding is annotated as `{}`, but the initializer resolves to `{}`",
                        declared_type.display_name(),
                        value.ty.display_name()
                    ),
                    source_path,
                );
            }
            let binding_type = combine_declared_and_actual_type(&declared_type, &value.ty);
            scopes.define_current(
                name.clone(),
                LocalBinding {
                    mutable: *mutable,
                    ty: binding_type,
                },
            );

            TypedStmt::Bind {
                name: name.clone(),
                mutable: *mutable,
                value,
            }
        }
        HirStmt::Assign { name, value, span } => {
            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            if let Some(existing) = scopes.get_mut(name) {
                if !existing.mutable {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3003",
                            format!("cannot reassign immutable binding `{name}`"),
                            "bindings declared without `mut` are immutable after their first assignment",
                            *span,
                        )
                        .with_fix_it("declare the binding as `mut name = ...` before reassigning it")
                        .with_source_path(source_path.to_path_buf()),
                    );
                } else {
                    ensure_type_compatibility(
                        &existing.ty,
                        &value.ty,
                        *span,
                        diagnostics,
                        format!("assignment to `{name}` violates its type"),
                        format!(
                            "the binding holds `{}`, but the assigned expression resolves to `{}`",
                            existing.ty.display_name(),
                            value.ty.display_name()
                        ),
                        source_path,
                    );
                    existing.ty = combine_declared_and_actual_type(&existing.ty, &value.ty);
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
                        ty: value.ty.clone(),
                    },
                );
                TypedStmt::Bind {
                    name: name.clone(),
                    mutable: false,
                    value,
                }
            }
        }
        HirStmt::Break(span) => {
            validate_loop_control("break", *span, loop_depth, diagnostics, source_path);
            TypedStmt::Break
        }
        HirStmt::Continue(span) => {
            validate_loop_control("continue", *span, loop_depth, diagnostics, source_path);
            TypedStmt::Continue
        }
        HirStmt::If {
            condition,
            then_body,
            else_body,
            span: _,
        } => {
            let condition = lower_expr(
                condition,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            ensure_bool_condition(&condition, diagnostics, source_path);
            let then_body = lower_block(
                then_body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                true,
                function_return_type,
                source_path,
            );
            let else_body = lower_block(
                else_body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                true,
                function_return_type,
                source_path,
            );
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
            let condition = lower_expr(
                condition,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            ensure_bool_condition(&condition, diagnostics, source_path);
            let body = lower_block(
                body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth + 1,
                true,
                function_return_type,
                source_path,
            );
            TypedStmt::While { condition, body }
        }
        HirStmt::For {
            binding,
            iterable,
            body,
            span,
        } => {
            let iterable = lower_expr(
                iterable,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let binding_type =
                resolve_for_binding_type(binding, &iterable, *span, diagnostics, source_path);
            scopes.push();
            scopes.define_current(
                binding.clone(),
                LocalBinding {
                    mutable: false,
                    ty: binding_type,
                },
            );
            let body = lower_block(
                body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth + 1,
                false,
                function_return_type,
                source_path,
            );
            scopes.pop();
            TypedStmt::For {
                binding: binding.clone(),
                iterable,
                body,
            }
        }
        HirStmt::Match { value, arms, span } => {
            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let arms = lower_match_arms(
                arms,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                function_return_type,
                source_path,
                &value,
                *span,
            );
            TypedStmt::Match { value, arms }
        }
        HirStmt::Select { arms, span } => {
            let arms = lower_select_arms(
                arms,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                function_return_type,
                source_path,
                *span,
            );
            TypedStmt::Select { arms }
        }
        HirStmt::Expr(expr, _) => TypedStmt::Expr(lower_expr(
            expr,
            scopes,
            signatures,
            method_signatures,
            known_structs,
            known_enums,
            struct_signatures,
            enum_signatures,
            diagnostics,
            function_return_type,
            source_path,
        )),
    }
}

fn lower_match_arms(
    arms: &[HirMatchArm],
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    loop_depth: usize,
    function_return_type: &Type,
    source_path: &Path,
    value: &TypedExpr,
    span: Span,
) -> Vec<TypedMatchArm> {
    let match_target = match &value.ty {
        Type::Enum(name) => Some(MatchTargetKind::Enum(name.clone())),
        Type::Result(ok, err) => Some(MatchTargetKind::Result {
            ok: ok.as_ref().clone(),
            err: err.as_ref().clone(),
        }),
        Type::Unknown => None,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3032",
                    "`match` currently requires an enum value",
                    format!(
                        "this match target resolves to `{}` instead of an enum-like value",
                        other.display_name()
                    ),
                    value.span,
                )
                .with_fix_it("match over an enum value or a `Result[T, E]` expression")
                .with_source_path(source_path.to_path_buf()),
            );
            None
        }
    };

    let mut seen_variants = HashSet::new();
    let typed_arms = arms
        .iter()
        .map(|arm| {
            let (pattern, bindings) = lower_match_pattern(
                &arm.pattern,
                match_target.as_ref(),
                &mut seen_variants,
                enum_signatures,
                diagnostics,
                source_path,
            );
            scopes.push();
            for binding in &bindings {
                if scopes.contains_in_current(&binding.name) {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3006",
                            format!("duplicate binding `{}`", binding.name),
                            "payload match bindings must be unique within the same match arm",
                            arm.span,
                        )
                        .with_fix_it("rename or remove the duplicate payload binding")
                        .with_source_path(source_path.to_path_buf()),
                    );
                } else {
                    scopes.define_current(
                        binding.name.clone(),
                        LocalBinding {
                            mutable: false,
                            ty: binding.ty.clone(),
                        },
                    );
                }
            }
            let body = lower_block(
                &arm.body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                false,
                function_return_type,
                source_path,
            );
            scopes.pop();
            TypedMatchArm { pattern, body }
        })
        .collect::<Vec<_>>();

    if let Some(match_target) = match_target {
        ensure_match_exhaustive(
            &match_target,
            &seen_variants,
            enum_signatures,
            diagnostics,
            span,
            source_path,
        );
    }

    typed_arms
}

fn lower_select_arms(
    arms: &[HirSelectArm],
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    loop_depth: usize,
    function_return_type: &Type,
    source_path: &Path,
    span: Span,
) -> Vec<TypedSelectArm> {
    if arms.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3047",
                "`select` requires at least one arm",
                "the bootstrap select model needs one or more `recv(channel)`, `send(channel, value)`, or `default` arms",
                span,
            )
            .with_fix_it("add a select arm like `value = recv(ch):`, `send(ch, value):`, or `default:`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    let mut typed_arms = Vec::with_capacity(arms.len());
    let mut saw_default = false;

    for arm in arms {
        let (kind, binding_ty) = match &arm.kind {
            HirSelectArmKind::Operation { operation } => {
                let operation = lower_expr(
                    operation,
                    scopes,
                    signatures,
                    method_signatures,
                    known_structs,
                    known_enums,
                    struct_signatures,
                    enum_signatures,
                    diagnostics,
                    function_return_type,
                    source_path,
                );
                let select_kind = validate_select_operation(&operation, diagnostics, source_path);
                let ty = operation.ty.clone();
                let kind = match select_kind {
                    Some(SelectOperationKind::Recv) => TypedSelectArmKind::Recv { operation },
                    Some(SelectOperationKind::Send) => TypedSelectArmKind::Send { operation },
                    None => TypedSelectArmKind::Recv { operation },
                };
                (kind, Some(ty))
            }
            HirSelectArmKind::Default => {
                if saw_default {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3095",
                            "`select` allows only one `default` arm",
                            "multiple `default` arms would make the immediate fallback path ambiguous",
                            arm.span,
                        )
                        .with_fix_it("remove the duplicate `default` arm or merge its body into the first one")
                        .with_source_path(source_path.to_path_buf()),
                    );
                }
                saw_default = true;
                (TypedSelectArmKind::Default, None)
            }
        };

        let body = {
            scopes.push();
            if let (Some(binding), Some(binding_ty)) = (&arm.binding, &binding_ty) {
                if scopes.contains_in_current(binding) {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3006",
                            format!("duplicate binding `{binding}`"),
                            "gof currently does not allow duplicate bindings in the same block scope",
                            arm.span,
                        )
                        .with_fix_it("rename the select arm binding")
                        .with_source_path(source_path.to_path_buf()),
                    );
                } else {
                    scopes.define_current(
                        binding.clone(),
                        LocalBinding {
                            mutable: false,
                            ty: binding_ty.clone(),
                        },
                    );
                }
            }
            let body = lower_block(
                &arm.body,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                loop_depth,
                false,
                function_return_type,
                source_path,
            );
            scopes.pop();
            body
        };

        typed_arms.push(TypedSelectArm {
            binding: arm.binding.clone(),
            kind,
            body,
        });
    }

    typed_arms
}

fn resolve_for_binding_type(
    binding: &str,
    iterable: &TypedExpr,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    match &iterable.ty {
        Type::List(item) => item.as_ref().clone(),
        Type::String => Type::String,
        Type::Dict(_) => Type::String,
        Type::Unknown => Type::Unknown,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3048",
                    format!("`for {binding} in ...` requires an iterable value"),
                    format!(
                        "this loop target resolves to `{}`, but bootstrap `for` currently supports only `list`, `string`, and `dict`",
                        other.display_name()
                    ),
                    span,
                )
                .with_fix_it("iterate over a list, string, or dict value")
                .with_source_path(source_path.to_path_buf()),
            );
            Type::Unknown
        }
    }
}

fn lower_match_pattern(
    pattern: &HirMatchPattern,
    match_target: Option<&MatchTargetKind>,
    seen_variants: &mut HashSet<String>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> (TypedMatchPattern, Vec<TypedMatchBinding>) {
    let Some(match_target) = match_target else {
        return (fallback_match_pattern(pattern), Vec::new());
    };

    let HirMatchPattern::EnumVariant {
        enum_name,
        variant,
        bindings,
        span,
    } = pattern;

    if enum_name != match_target.display_name() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3031",
                "match arm uses a variant from a different enum",
                format!(
                    "this match targets `{}`, but the arm pattern belongs to `{enum_name}`",
                    match_target.display_name()
                ),
                *span,
            )
            .with_fix_it(format!(
                "use a `{}.Variant` pattern here",
                match_target.display_name()
            ))
            .with_source_path(source_path.to_path_buf()),
        );
        return (fallback_match_pattern(pattern), Vec::new());
    }

    let variant_fields = match match_target {
        MatchTargetKind::Enum(expected_enum) => {
            let Some(signature) = enum_signatures.get(expected_enum) else {
                return (fallback_match_pattern(pattern), Vec::new());
            };

            let Some(variant_signature) = signature
                .variants
                .iter()
                .find(|candidate| candidate.name == *variant)
            else {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3028",
                        format!("unknown variant `{variant}` on `{enum_name}`"),
                        "enum match patterns must use a variant declared on the enum",
                        *span,
                    )
                    .with_fix_it("use one of the variants declared on the enum")
                    .with_source_path(source_path.to_path_buf()),
                );
                return (fallback_match_pattern(pattern), Vec::new());
            };
            variant_signature.fields.clone()
        }
        MatchTargetKind::Result { ok, err } => match variant.as_str() {
            "Ok" => vec![EnumVariantFieldSignature {
                name: "value".to_string(),
                ty: ok.clone(),
            }],
            "Err" => vec![EnumVariantFieldSignature {
                name: "error".to_string(),
                ty: err.clone(),
            }],
            _ => {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3072",
                        format!("unknown result variant `Result.{variant}`"),
                        "the builtin result type exposes only `Result.Ok(value)` and `Result.Err(error)`",
                        *span,
                    )
                    .with_fix_it("use `Result.Ok(value)` or `Result.Err(error)`")
                    .with_source_path(source_path.to_path_buf()),
                );
                return (fallback_match_pattern(pattern), Vec::new());
            }
        },
    };

    if !seen_variants.insert(variant.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3030",
                format!(
                    "duplicate match arm for `{}.{variant}`",
                    match_target.display_name()
                ),
                "each variant can appear only once in a match over the same value space",
                *span,
            )
            .with_fix_it("remove the duplicate arm or replace it with another enum variant")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if bindings.len() != variant_fields.len() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3071",
                format!(
                    "match arm for `{enum_name}.{variant}` destructures the wrong number of payload values"
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
                variant,
                if variant_fields.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        "({})",
                        variant_fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ))
            .with_source_path(source_path.to_path_buf()),
        );
    }

    let typed_bindings = bindings
        .iter()
        .zip(variant_fields.iter())
        .map(|(binding, field)| TypedMatchBinding {
            name: binding.clone(),
            ty: field.ty.clone(),
        })
        .collect::<Vec<_>>();

    (
        TypedMatchPattern::EnumVariant {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            bindings: typed_bindings.clone(),
            span: *span,
        },
        typed_bindings,
    )
}

fn ensure_match_exhaustive(
    match_target: &MatchTargetKind,
    seen_variants: &HashSet<String>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    span: Span,
    source_path: &Path,
) {
    let missing = match match_target {
        MatchTargetKind::Enum(enum_name) => {
            let Some(signature) = enum_signatures.get(enum_name) else {
                return;
            };
            signature
                .variants
                .iter()
                .filter(|variant| !seen_variants.contains(&variant.name))
                .map(|variant| variant.name.clone())
                .collect::<Vec<_>>()
        }
        MatchTargetKind::Result { .. } => ["Ok".to_string(), "Err".to_string()]
            .into_iter()
            .filter(|variant| !seen_variants.contains(variant))
            .collect::<Vec<_>>(),
    };

    if !missing.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3033",
                format!(
                    "non-exhaustive match over `{}`",
                    match_target.display_name()
                ),
                format!(
                    "missing arm(s): {}",
                    missing
                        .iter()
                        .map(|variant| format!("{}.{variant}", match_target.display_name()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                span,
            )
            .with_fix_it("add match arms for every remaining enum variant")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn fallback_match_pattern(pattern: &HirMatchPattern) -> TypedMatchPattern {
    match pattern {
        HirMatchPattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            span,
        } => TypedMatchPattern::EnumVariant {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            bindings: bindings
                .iter()
                .map(|binding| TypedMatchBinding {
                    name: binding.clone(),
                    ty: Type::Unknown,
                })
                .collect(),
            span: *span,
        },
    }
}

fn ensure_bool_condition(condition: &TypedExpr, diagnostics: &mut Diagnostics, source_path: &Path) {
    if !matches!(condition.ty, Type::Bool | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3007",
                "condition must evaluate to `bool`",
                "control-flow conditions in gof currently require a boolean expression",
                condition.span,
            )
            .with_fix_it("use a comparison like `x > 0` or a boolean literal")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_loop_control(
    keyword: &str,
    span: Span,
    loop_depth: usize,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if loop_depth > 0 {
        return;
    }

    let (code, note) = match keyword {
        "break" => (
            "GOF3053",
            "`break` currently works only inside `while` and `for` loop bodies",
        ),
        "continue" => (
            "GOF3054",
            "`continue` currently works only inside `while` and `for` loop bodies",
        ),
        _ => unreachable!("loop control validator supports only break and continue"),
    };

    diagnostics.push(
        Diagnostic::error(
            code,
            format!("`{keyword}` is only valid inside a loop"),
            note,
            span,
        )
        .with_fix_it("move this statement into a surrounding `while` or `for` loop")
        .with_source_path(source_path.to_path_buf()),
    );
}

fn lower_expr(
    expr: &HirExpr,
    scopes: &ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    function_return_type: &Type,
    source_path: &Path,
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
        HirExpr::List { items, span } => {
            let typed_items = items
                .iter()
                .map(|item| {
                    lower_expr(
                        item,
                        scopes,
                        signatures,
                        method_signatures,
                        known_structs,
                        known_enums,
                        struct_signatures,
                        enum_signatures,
                        diagnostics,
                        function_return_type,
                        source_path,
                    )
                })
                .collect::<Vec<_>>();
            let element_type = infer_list_element_type(&typed_items, diagnostics, source_path);
            TypedExpr {
                kind: TypedExprKind::List { items: typed_items },
                ty: Type::list(element_type),
                span: *span,
            }
        }
        HirExpr::Dict { entries, span } => {
            let typed_entries = entries
                .iter()
                .map(|entry| TypedDictEntry {
                    key: lower_expr(
                        &entry.key,
                        scopes,
                        signatures,
                        method_signatures,
                        known_structs,
                        known_enums,
                        struct_signatures,
                        enum_signatures,
                        diagnostics,
                        function_return_type,
                        source_path,
                    ),
                    value: lower_expr(
                        &entry.value,
                        scopes,
                        signatures,
                        method_signatures,
                        known_structs,
                        known_enums,
                        struct_signatures,
                        enum_signatures,
                        diagnostics,
                        function_return_type,
                        source_path,
                    ),
                })
                .collect::<Vec<_>>();
            let value_type = infer_dict_value_type(&typed_entries, diagnostics, source_path);
            TypedExpr {
                kind: TypedExprKind::Dict {
                    entries: typed_entries,
                },
                ty: Type::dict(value_type),
                span: *span,
            }
        }
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
                    .with_fix_it("define the binding before using it")
                    .with_source_path(source_path.to_path_buf()),
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
                .map(|arg| {
                    lower_expr(
                        arg,
                        scopes,
                        signatures,
                        method_signatures,
                        known_structs,
                        known_enums,
                        struct_signatures,
                        enum_signatures,
                        diagnostics,
                        function_return_type,
                        source_path,
                    )
                })
                .collect::<Vec<_>>();
            let call_kind =
                resolve_call_kind(callee, signatures, struct_signatures, enum_signatures);
            validate_call(
                callee,
                &typed_args,
                *span,
                call_kind,
                signatures,
                struct_signatures,
                diagnostics,
                source_path,
            );
            let return_type = call_return_type(
                callee,
                call_kind,
                &typed_args,
                signatures,
                struct_signatures,
            );

            TypedExpr {
                kind: match call_kind {
                    CallKind::Function
                    | CallKind::BuiltinLen
                    | CallKind::BuiltinPrint
                    | CallKind::BuiltinAppend
                    | CallKind::BuiltinContains
                    | CallKind::BuiltinFirst
                    | CallKind::BuiltinLast
                    | CallKind::BuiltinSlice
                    | CallKind::BuiltinReverse
                    | CallKind::BuiltinSort
                    | CallKind::BuiltinMin
                    | CallKind::BuiltinMax
                    | CallKind::BuiltinTrim
                    | CallKind::BuiltinSplit
                    | CallKind::BuiltinJoin
                    | CallKind::BuiltinStartsWith
                    | CallKind::BuiltinEndsWith
                    | CallKind::BuiltinParseInt
                    | CallKind::BuiltinToString
                    | CallKind::BuiltinRange
                    | CallKind::BuiltinSleep
                    | CallKind::BuiltinAssert
                    | CallKind::BuiltinArgv
                    | CallKind::BuiltinEnv
                    | CallKind::BuiltinCwd
                    | CallKind::BuiltinExists
                    | CallKind::BuiltinReadDir
                    | CallKind::BuiltinMkdir
                    | CallKind::BuiltinRemoveFile
                    | CallKind::BuiltinPathJoin
                    | CallKind::BuiltinPathDir
                    | CallKind::BuiltinPathBase
                    | CallKind::BuiltinPathExt
                    | CallKind::BuiltinReadFile
                    | CallKind::BuiltinWriteFile
                    | CallKind::BuiltinDict
                    | CallKind::BuiltinInsert
                    | CallKind::BuiltinKeys
                    | CallKind::BuiltinValues
                    | CallKind::BuiltinChannel
                    | CallKind::BuiltinClose
                    | CallKind::BuiltinSend
                    | CallKind::BuiltinRecv
                    | CallKind::BuiltinCancelToken
                    | CallKind::BuiltinCancel
                    | CallKind::BuiltinIsCancelled
                    | CallKind::BuiltinTimeoutToken
                    | CallKind::BuiltinCancelAfter
                    | CallKind::BuiltinJsonParse
                    | CallKind::BuiltinJsonStringify
                    | CallKind::BuiltinJsonGet
                    | CallKind::BuiltinJsonIndex
                    | CallKind::BuiltinJsonLen
                    | CallKind::BuiltinJsonString
                    | CallKind::BuiltinJsonInt
                    | CallKind::BuiltinHttpGet
                    | CallKind::BuiltinHttpPost
                    | CallKind::Enum
                    | CallKind::Unknown => TypedExprKind::Call {
                        callee: callee.clone(),
                        args: typed_args,
                    },
                    CallKind::Struct => TypedExprKind::StructInit {
                        name: callee.clone(),
                        args: typed_args,
                    },
                },
                ty: return_type,
                span: *span,
            }
        }
        HirExpr::Field {
            target,
            field,
            span,
        } => {
            if let HirExpr::Local(name, target_span) = target.as_ref() {
                if scopes.get(name).is_none() && name == "Result" {
                    let ty =
                        resolve_result_variant_reference(field, diagnostics, *span, source_path);
                    return TypedExpr {
                        kind: TypedExprKind::EnumVariant {
                            enum_name: name.clone(),
                            variant: field.clone(),
                            args: Vec::new(),
                        },
                        ty,
                        span: Span::new(target_span.line, target_span.column, span.end_column),
                    };
                }
                if scopes.get(name).is_none() && enum_signatures.contains_key(name) {
                    let (ty, payload_fields) = resolve_enum_variant_reference(
                        name,
                        field,
                        diagnostics,
                        *span,
                        enum_signatures,
                        source_path,
                    );
                    let args = if payload_fields.is_empty() {
                        Vec::new()
                    } else {
                        diagnostics.push(
                            Diagnostic::error(
                                "GOF3069",
                                format!("enum variant `{name}.{field}` requires payload values"),
                                format!(
                                    "this variant expects {} payload value(s)",
                                    payload_fields.len()
                                ),
                                *span,
                            )
                            .with_fix_it(format!(
                                "construct it as `{}.{}`({})",
                                name,
                                field,
                                payload_fields
                                    .iter()
                                    .map(|payload| payload.name.clone())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                            .with_source_path(source_path.to_path_buf()),
                        );
                        Vec::new()
                    };
                    return TypedExpr {
                        kind: TypedExprKind::EnumVariant {
                            enum_name: name.clone(),
                            variant: field.clone(),
                            args,
                        },
                        ty,
                        span: Span::new(target_span.line, target_span.column, span.end_column),
                    };
                }
            }

            let target = lower_expr(
                target,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let ty = infer_field_type(
                &target,
                field,
                diagnostics,
                *span,
                struct_signatures,
                source_path,
            );
            TypedExpr {
                kind: TypedExprKind::Field {
                    target: Box::new(target),
                    field: field.clone(),
                },
                ty,
                span: *span,
            }
        }
        HirExpr::MethodCall {
            target,
            method,
            args,
            span,
        } => {
            if let HirExpr::Local(name, target_span) = target.as_ref() {
                if scopes.get(name).is_none() && name == "Result" {
                    let typed_args = args
                        .iter()
                        .map(|arg| {
                            lower_expr(
                                arg,
                                scopes,
                                signatures,
                                method_signatures,
                                known_structs,
                                known_enums,
                                struct_signatures,
                                enum_signatures,
                                diagnostics,
                                function_return_type,
                                source_path,
                            )
                        })
                        .collect::<Vec<_>>();
                    let ty = resolve_result_variant_call(
                        method,
                        &typed_args,
                        *span,
                        diagnostics,
                        source_path,
                    );
                    return TypedExpr {
                        kind: TypedExprKind::EnumVariant {
                            enum_name: name.clone(),
                            variant: method.clone(),
                            args: typed_args,
                        },
                        ty,
                        span: Span::new(target_span.line, target_span.column, span.end_column),
                    };
                }
                if scopes.get(name).is_none() && enum_signatures.contains_key(name) {
                    let typed_args = args
                        .iter()
                        .map(|arg| {
                            lower_expr(
                                arg,
                                scopes,
                                signatures,
                                method_signatures,
                                known_structs,
                                known_enums,
                                struct_signatures,
                                enum_signatures,
                                diagnostics,
                                function_return_type,
                                source_path,
                            )
                        })
                        .collect::<Vec<_>>();
                    let ty = resolve_enum_variant_call(
                        name,
                        method,
                        &typed_args,
                        *span,
                        enum_signatures,
                        diagnostics,
                        source_path,
                    );
                    return TypedExpr {
                        kind: TypedExprKind::EnumVariant {
                            enum_name: name.clone(),
                            variant: method.clone(),
                            args: typed_args,
                        },
                        ty,
                        span: Span::new(target_span.line, target_span.column, span.end_column),
                    };
                }
            }

            let target = lower_expr(
                target,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let typed_args = args
                .iter()
                .map(|arg| {
                    lower_expr(
                        arg,
                        scopes,
                        signatures,
                        method_signatures,
                        known_structs,
                        known_enums,
                        struct_signatures,
                        enum_signatures,
                        diagnostics,
                        function_return_type,
                        source_path,
                    )
                })
                .collect::<Vec<_>>();
            let (symbol_name, ty) = resolve_method_call(
                &target,
                method,
                &typed_args,
                *span,
                method_signatures,
                diagnostics,
                source_path,
            );
            TypedExpr {
                kind: TypedExprKind::MethodCall {
                    target: Box::new(target),
                    method: method.clone(),
                    symbol_name,
                    args: typed_args,
                },
                ty,
                span: *span,
            }
        }
        HirExpr::Index {
            target,
            index,
            span,
        } => {
            let target = lower_expr(
                target,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let index = lower_expr(
                index,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );

            let ty = infer_index_type(&target, &index, diagnostics, *span, source_path);
            TypedExpr {
                kind: TypedExprKind::Index {
                    target: Box::new(target),
                    index: Box::new(index),
                },
                ty,
                span: *span,
            }
        }
        HirExpr::Go { value, span } => match value.as_ref() {
            HirExpr::Call {
                callee,
                args,
                span: call_span,
            } => {
                let typed_args = args
                    .iter()
                    .map(|arg| {
                        lower_expr(
                            arg,
                            scopes,
                            signatures,
                            method_signatures,
                            known_structs,
                            known_enums,
                            struct_signatures,
                            enum_signatures,
                            diagnostics,
                            function_return_type,
                            source_path,
                        )
                    })
                    .collect::<Vec<_>>();
                let call_kind =
                    resolve_call_kind(callee, signatures, struct_signatures, enum_signatures);
                if !matches!(call_kind, CallKind::Function | CallKind::Unknown) {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3008",
                            "`go` currently requires a named function call",
                            "the bootstrap concurrency model only supports `go some_fn(...)` for top-level functions",
                            *span,
                        )
                        .with_fix_it("replace this expression with `go some_function(...)`")
                        .with_source_path(source_path.to_path_buf()),
                    );
                }
                validate_call(
                    callee,
                    &typed_args,
                    *call_span,
                    call_kind,
                    signatures,
                    struct_signatures,
                    diagnostics,
                    source_path,
                );
                let return_type = call_return_type(
                    callee,
                    call_kind,
                    &typed_args,
                    signatures,
                    struct_signatures,
                );

                TypedExpr {
                    kind: TypedExprKind::Spawn {
                        callee: callee.clone(),
                        args: typed_args,
                    },
                    ty: Type::task(return_type),
                    span: *span,
                }
            }
            other => {
                let typed_value = lower_expr(
                    other,
                    scopes,
                    signatures,
                    method_signatures,
                    known_structs,
                    known_enums,
                    struct_signatures,
                    enum_signatures,
                    diagnostics,
                    function_return_type,
                    source_path,
                );
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3008",
                        "`go` currently requires a named function call",
                        "the bootstrap concurrency model only supports `go some_fn(...)` for top-level functions",
                        *span,
                    )
                    .with_fix_it("replace this expression with `go some_function(...)`")
                    .with_source_path(source_path.to_path_buf()),
                );

                TypedExpr {
                    kind: TypedExprKind::Spawn {
                        callee: "_error".to_string(),
                        args: vec![typed_value],
                    },
                    ty: Type::task(Type::Unknown),
                    span: *span,
                }
            }
        },
        HirExpr::Await { value, span } => {
            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let ty = match &value.ty {
                Type::Task(inner) => inner.as_ref().clone(),
                Type::Unknown => Type::Unknown,
                _ => {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3009",
                            "`await` requires a task value",
                        "only values produced by `go` can currently be awaited in the bootstrap compiler",
                            *span,
                        )
                        .with_fix_it("store `go some_function(...)` in a binding and await that task")
                        .with_source_path(source_path.to_path_buf()),
                    );
                    Type::Unknown
                }
            };

            TypedExpr {
                kind: TypedExprKind::Await {
                    value: Box::new(value),
                },
                ty,
                span: *span,
            }
        }
        HirExpr::Propagate { value, span } => {
            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let ty = validate_propagate_expr(
                &value,
                function_return_type,
                *span,
                diagnostics,
                source_path,
            );
            TypedExpr {
                kind: TypedExprKind::Propagate {
                    value: Box::new(value),
                },
                ty,
                span: *span,
            }
        }
        HirExpr::Unary { op, value, span } => {
            let value = lower_expr(
                value,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            validate_unary_expr(*op, &value, diagnostics, source_path);
            let ty = infer_unary_type(*op, &value.ty);
            TypedExpr {
                kind: TypedExprKind::Unary {
                    op: *op,
                    value: Box::new(value),
                },
                ty,
                span: *span,
            }
        }
        HirExpr::Binary { lhs, op, rhs, span } => {
            let lhs = lower_expr(
                lhs,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            let rhs = lower_expr(
                rhs,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                function_return_type,
                source_path,
            );
            validate_binary_expr(
                *op,
                &lhs,
                &rhs,
                struct_signatures,
                enum_signatures,
                diagnostics,
                source_path,
            );
            let ty = infer_binary_type(&lhs.ty, *op, &rhs.ty, struct_signatures, enum_signatures);
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

fn validate_call(
    callee: &str,
    args: &[TypedExpr],
    span: Span,
    call_kind: CallKind,
    signatures: &HashMap<String, FunctionSignature>,
    struct_signatures: &HashMap<String, StructSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    match call_kind {
        CallKind::BuiltinLen => {
            validate_len_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinPrint => {
            validate_print_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinAppend => {
            validate_append_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinContains => {
            validate_contains_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinFirst => {
            validate_first_or_last_call("first", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinLast => {
            validate_first_or_last_call("last", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSlice => {
            validate_slice_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinReverse => {
            validate_reverse_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSort => {
            validate_sort_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinMin => {
            validate_min_or_max_call("min", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinMax => {
            validate_min_or_max_call("max", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinTrim => {
            validate_trim_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSplit => {
            validate_split_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJoin => {
            validate_join_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinStartsWith => {
            validate_starts_with_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinEndsWith => {
            validate_ends_with_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinParseInt => {
            validate_parse_int_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinToString => {
            validate_to_string_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinRange => {
            validate_range_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSleep => {
            validate_sleep_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinAssert => {
            validate_assert_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinArgv => {
            validate_no_argument_call("argv", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinEnv => {
            validate_env_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinCwd => {
            validate_no_argument_call("cwd", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinExists => {
            validate_single_string_argument_call("exists", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinReadDir => {
            validate_single_string_argument_call("read_dir", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinMkdir => {
            validate_single_string_argument_call("mkdir", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinRemoveFile => {
            validate_single_string_argument_call(
                "remove_file",
                args,
                span,
                diagnostics,
                source_path,
            );
        }
        CallKind::BuiltinPathJoin => {
            validate_two_string_argument_call("path_join", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinPathDir => {
            validate_single_string_argument_call("path_dir", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinPathBase => {
            validate_single_string_argument_call(
                "path_base",
                args,
                span,
                diagnostics,
                source_path,
            );
        }
        CallKind::BuiltinPathExt => {
            validate_single_string_argument_call("path_ext", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinReadFile => {
            validate_read_file_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinWriteFile => {
            validate_write_file_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinDict => {
            validate_dict_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinInsert => {
            validate_insert_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinKeys => {
            validate_keys_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinValues => {
            validate_values_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinChannel => {
            validate_channel_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinClose => {
            validate_close_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSend => {
            validate_send_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinRecv => {
            validate_recv_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinCancelToken => {
            validate_no_argument_call("cancel_token", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinCancel => {
            validate_cancel_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinIsCancelled => {
            validate_is_cancelled_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinTimeoutToken => {
            validate_timeout_token_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinCancelAfter => {
            validate_cancel_after_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonParse => {
            validate_single_string_argument_call(
                "json_parse",
                args,
                span,
                diagnostics,
                source_path,
            );
        }
        CallKind::BuiltinJsonStringify => {
            validate_json_stringify_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonGet => {
            validate_json_get_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonIndex => {
            validate_json_index_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonLen => {
            validate_json_len_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonString => {
            validate_json_string_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinJsonInt => {
            validate_json_int_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinHttpGet => {
            validate_single_string_argument_call("http_get", args, span, diagnostics, source_path);
        }
        CallKind::BuiltinHttpPost => {
            validate_http_post_call(args, span, diagnostics, source_path);
        }
        CallKind::Function => match signatures.get(callee) {
            Some(signature) if signature.arity == args.len() => {
                for (index, (expected, actual)) in
                    signature.param_types.iter().zip(args.iter()).enumerate()
                {
                    ensure_type_compatibility(
                        expected,
                        &actual.ty,
                        actual.span,
                        diagnostics,
                        format!("argument {} for `{callee}` has incompatible type", index + 1),
                        format!(
                            "parameter expects `{}`, but the argument resolves to `{}`",
                            expected.display_name(),
                            actual.ty.display_name()
                        ),
                        source_path,
                    );
                }
            }
            Some(signature) => diagnostics.push(
                Diagnostic::error(
                    "GOF3005",
                    format!("wrong number of arguments for `{callee}`"),
                    format!("expected {} argument(s), got {}", signature.arity, args.len()),
                    span,
                )
                .with_fix_it("pass the exact number of parameters declared by the function")
                .with_source_path(source_path.to_path_buf()),
            ),
            None => unreachable!("function call kind requires a matching signature"),
        },
        CallKind::Struct => {
            let signature = struct_signatures
                .get(callee)
                .expect("struct call kind requires a matching signature");
            if signature.fields.len() != args.len() {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3005",
                        format!("wrong number of arguments for `{callee}`"),
                        format!(
                            "expected {} field value(s), got {}",
                            signature.fields.len(),
                            args.len()
                        ),
                        span,
                    )
                    .with_fix_it("pass one argument for each struct field in declaration order")
                    .with_source_path(source_path.to_path_buf()),
                );
                return;
            }

            for (index, (expected, actual)) in
                signature.fields.iter().zip(args.iter()).enumerate()
            {
                ensure_type_compatibility(
                    &expected.ty,
                    &actual.ty,
                    actual.span,
                    diagnostics,
                    format!("field {} for `{callee}` has incompatible type", index + 1),
                    format!(
                        "field `{}` expects `{}`, but the argument resolves to `{}`",
                        expected.name,
                        expected.ty.display_name(),
                        actual.ty.display_name()
                    ),
                    source_path,
                );
            }
        }
        CallKind::Enum => diagnostics.push(
            Diagnostic::error(
                "GOF3004",
                format!("enum `{callee}` is not callable"),
                "unit enum variants use `EnumName.Variant`, not constructor calls",
                span,
            )
            .with_fix_it("replace this call with `EnumName.Variant`")
            .with_source_path(source_path.to_path_buf()),
        ),
        CallKind::Unknown => diagnostics.push(
                Diagnostic::error(
                    "GOF3004",
                    format!("unknown function or struct `{callee}`"),
                    "calls currently resolve only to top-level functions, supported builtin helpers, or struct constructors; enums use `EnumName.Variant`",
                    span,
                )
            .with_fix_it("define the function or struct before calling it")
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn resolve_method_call(
    target: &TypedExpr,
    method: &str,
    args: &[TypedExpr],
    span: Span,
    method_signatures: &HashMap<(String, String), FunctionSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> (String, Type) {
    let receiver_name = match &target.ty {
        Type::Struct(name) => Some(name.clone()),
        Type::Unknown => None,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3037",
                    format!("method call `{method}` requires a struct receiver"),
                    format!("this target resolves to `{}`", other.display_name()),
                    target.span,
                )
                .with_fix_it("call methods only on struct values")
                .with_source_path(source_path.to_path_buf()),
            );
            None
        }
    };

    let Some(receiver_name) = receiver_name else {
        return (format!("_error.{method}"), Type::Unknown);
    };

    let Some(signature) = method_signatures.get(&(receiver_name.clone(), method.to_string()))
    else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3036",
                format!("unknown method `{method}` on `{receiver_name}`"),
                "method calls currently resolve only to receiver methods declared as `fn TypeName.method(...)`",
                span,
            )
            .with_fix_it("declare the method on the struct or call an existing method name")
            .with_source_path(source_path.to_path_buf()),
        );
        return (format!("{receiver_name}.{method}"), Type::Unknown);
    };

    let expected_arity = signature.arity.saturating_sub(1);
    if expected_arity != args.len() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{receiver_name}.{method}`"),
                format!("expected {expected_arity} argument(s), got {}", args.len()),
                span,
            )
            .with_fix_it("pass the exact number of parameters declared after the receiver")
            .with_source_path(source_path.to_path_buf()),
        );
    } else {
        for (index, (expected, actual)) in signature
            .param_types
            .iter()
            .skip(1)
            .zip(args.iter())
            .enumerate()
        {
            ensure_type_compatibility(
                expected,
                &actual.ty,
                actual.span,
                diagnostics,
                format!(
                    "argument {} for `{receiver_name}.{method}` has incompatible type",
                    index + 1
                ),
                format!(
                    "parameter expects `{}`, but the argument resolves to `{}`",
                    expected.display_name(),
                    actual.ty.display_name()
                ),
                source_path,
            );
        }
    }

    (signature.symbol_name.clone(), signature.return_type.clone())
}

fn resolve_type_annotation(
    ty: Option<&HirTypeRef>,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    source_path: &Path,
    diagnostics: &mut Diagnostics,
) -> Type {
    let Some(ty) = ty else {
        return Type::Unknown;
    };

    match ty.name.as_str() {
        "int" => resolve_non_parameterized_builtin(
            "int",
            &ty.args,
            Type::Int,
            ty.span,
            diagnostics,
            source_path,
        ),
        "string" => resolve_non_parameterized_builtin(
            "string",
            &ty.args,
            Type::String,
            ty.span,
            diagnostics,
            source_path,
        ),
        "bool" => resolve_non_parameterized_builtin(
            "bool",
            &ty.args,
            Type::Bool,
            ty.span,
            diagnostics,
            source_path,
        ),
        "json" => resolve_non_parameterized_builtin(
            "json",
            &ty.args,
            Type::Json,
            ty.span,
            diagnostics,
            source_path,
        ),
        "cancel_token" => resolve_non_parameterized_builtin(
            "cancel_token",
            &ty.args,
            Type::CancelToken,
            ty.span,
            diagnostics,
            source_path,
        ),
        "unit" => resolve_non_parameterized_builtin(
            "unit",
            &ty.args,
            Type::Unit,
            ty.span,
            diagnostics,
            source_path,
        ),
        "list" => resolve_single_argument_type(
            "list",
            &ty.args,
            Type::list,
            known_structs,
            known_enums,
            source_path,
            diagnostics,
            ty.span,
        ),
        "dict" => resolve_single_argument_type(
            "dict",
            &ty.args,
            Type::dict,
            known_structs,
            known_enums,
            source_path,
            diagnostics,
            ty.span,
        ),
        "channel" => resolve_single_argument_type(
            "channel",
            &ty.args,
            Type::channel,
            known_structs,
            known_enums,
            source_path,
            diagnostics,
            ty.span,
        ),
        "task" => resolve_single_argument_type(
            "task",
            &ty.args,
            Type::task,
            known_structs,
            known_enums,
            source_path,
            diagnostics,
            ty.span,
        ),
        "Result" => resolve_result_type(
            &ty.args,
            known_structs,
            known_enums,
            source_path,
            diagnostics,
            ty.span,
        ),
        name if known_structs.contains(name) => resolve_named_type_without_args(
            name,
            &ty.args,
            Type::Struct(name.to_string()),
            ty.span,
            diagnostics,
            source_path,
        ),
        name if known_enums.contains(name) => resolve_named_type_without_args(
            name,
            &ty.args,
            Type::Enum(name.to_string()),
            ty.span,
            diagnostics,
            source_path,
        ),
        _ => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3012",
                    format!("unknown type annotation `{}`", ty.name),
                    "the bootstrap type system currently supports builtin annotations plus known struct and enum names from the loaded module graph",
                    ty.span,
                )
                .with_fix_it("replace the annotation with a supported builtin type or a known struct or enum name")
                .with_source_path(source_path.to_path_buf()),
            );
            Type::Unknown
        }
    }
}

fn resolve_non_parameterized_builtin(
    name: &str,
    args: &[HirTypeRef],
    ty: Type,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    if args.is_empty() {
        ty
    } else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3067",
                format!("`{name}` does not take type arguments"),
                format!("remove the `[ ... ]` from `{name}`"),
                span,
            )
            .with_fix_it(format!("use `{name}` without type arguments"))
            .with_source_path(source_path.to_path_buf()),
        );
        Type::Unknown
    }
}

fn resolve_named_type_without_args(
    name: &str,
    args: &[HirTypeRef],
    ty: Type,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    if args.is_empty() {
        ty
    } else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3067",
                format!("`{name}` does not support type arguments in the bootstrap type system"),
                "only builtin container and task annotations are parameterized in this language stage",
                span,
            )
            .with_fix_it(format!("remove the type arguments from `{name}`"))
            .with_source_path(source_path.to_path_buf()),
        );
        Type::Unknown
    }
}

fn resolve_single_argument_type(
    name: &str,
    args: &[HirTypeRef],
    constructor: fn(Type) -> Type,
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    source_path: &Path,
    diagnostics: &mut Diagnostics,
    span: Span,
) -> Type {
    if args.is_empty() {
        return constructor(Type::Unknown);
    }

    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3067",
                format!("`{name}` requires exactly one type argument"),
                format!("use `{name}[T]` with one inner type"),
                span,
            )
            .with_fix_it(format!("rewrite this annotation as `{name}[some_type]`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return Type::Unknown;
    }

    constructor(resolve_type_annotation(
        Some(&args[0]),
        known_structs,
        known_enums,
        source_path,
        diagnostics,
    ))
}

fn resolve_result_type(
    args: &[HirTypeRef],
    known_structs: &HashSet<String>,
    known_enums: &HashSet<String>,
    source_path: &Path,
    diagnostics: &mut Diagnostics,
    span: Span,
) -> Type {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3067",
                "`Result` requires exactly two type arguments",
                "use `Result[T, E]` with an ok type and an error type",
                span,
            )
            .with_fix_it("rewrite this annotation as `Result[some_value_type, some_error_type]`")
            .with_source_path(source_path.to_path_buf()),
        );
        return Type::Unknown;
    }

    Type::result(
        resolve_type_annotation(
            Some(&args[0]),
            known_structs,
            known_enums,
            source_path,
            diagnostics,
        ),
        resolve_type_annotation(
            Some(&args[1]),
            known_structs,
            known_enums,
            source_path,
            diagnostics,
        ),
    )
}

fn resolve_call_kind(
    callee: &str,
    signatures: &HashMap<String, FunctionSignature>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
) -> CallKind {
    if signatures.contains_key(callee) {
        CallKind::Function
    } else if callee == "len" {
        CallKind::BuiltinLen
    } else if callee == "print" {
        CallKind::BuiltinPrint
    } else if callee == "append" {
        CallKind::BuiltinAppend
    } else if callee == "contains" {
        CallKind::BuiltinContains
    } else if callee == "first" {
        CallKind::BuiltinFirst
    } else if callee == "last" {
        CallKind::BuiltinLast
    } else if callee == "slice" {
        CallKind::BuiltinSlice
    } else if callee == "reverse" {
        CallKind::BuiltinReverse
    } else if callee == "sort" {
        CallKind::BuiltinSort
    } else if callee == "min" {
        CallKind::BuiltinMin
    } else if callee == "max" {
        CallKind::BuiltinMax
    } else if callee == "trim" {
        CallKind::BuiltinTrim
    } else if callee == "split" {
        CallKind::BuiltinSplit
    } else if callee == "join" {
        CallKind::BuiltinJoin
    } else if callee == "starts_with" {
        CallKind::BuiltinStartsWith
    } else if callee == "ends_with" {
        CallKind::BuiltinEndsWith
    } else if callee == "parse_int" {
        CallKind::BuiltinParseInt
    } else if callee == "to_string" {
        CallKind::BuiltinToString
    } else if callee == "range" {
        CallKind::BuiltinRange
    } else if callee == "sleep" {
        CallKind::BuiltinSleep
    } else if callee == "assert" {
        CallKind::BuiltinAssert
    } else if callee == "argv" {
        CallKind::BuiltinArgv
    } else if callee == "env" {
        CallKind::BuiltinEnv
    } else if callee == "cwd" {
        CallKind::BuiltinCwd
    } else if callee == "exists" {
        CallKind::BuiltinExists
    } else if callee == "read_dir" {
        CallKind::BuiltinReadDir
    } else if callee == "mkdir" {
        CallKind::BuiltinMkdir
    } else if callee == "remove_file" {
        CallKind::BuiltinRemoveFile
    } else if callee == "path_join" {
        CallKind::BuiltinPathJoin
    } else if callee == "path_dir" {
        CallKind::BuiltinPathDir
    } else if callee == "path_base" {
        CallKind::BuiltinPathBase
    } else if callee == "path_ext" {
        CallKind::BuiltinPathExt
    } else if callee == "read_file" {
        CallKind::BuiltinReadFile
    } else if callee == "write_file" {
        CallKind::BuiltinWriteFile
    } else if callee == "dict" {
        CallKind::BuiltinDict
    } else if callee == "insert" {
        CallKind::BuiltinInsert
    } else if callee == "keys" {
        CallKind::BuiltinKeys
    } else if callee == "values" {
        CallKind::BuiltinValues
    } else if callee == "channel" {
        CallKind::BuiltinChannel
    } else if callee == "close" {
        CallKind::BuiltinClose
    } else if callee == "send" {
        CallKind::BuiltinSend
    } else if callee == "recv" {
        CallKind::BuiltinRecv
    } else if callee == "cancel_token" {
        CallKind::BuiltinCancelToken
    } else if callee == "cancel" {
        CallKind::BuiltinCancel
    } else if callee == "is_cancelled" {
        CallKind::BuiltinIsCancelled
    } else if callee == "timeout_token" {
        CallKind::BuiltinTimeoutToken
    } else if callee == "cancel_after" {
        CallKind::BuiltinCancelAfter
    } else if callee == "json_parse" {
        CallKind::BuiltinJsonParse
    } else if callee == "json_stringify" {
        CallKind::BuiltinJsonStringify
    } else if callee == "json_get" {
        CallKind::BuiltinJsonGet
    } else if callee == "json_index" {
        CallKind::BuiltinJsonIndex
    } else if callee == "json_len" {
        CallKind::BuiltinJsonLen
    } else if callee == "json_string" {
        CallKind::BuiltinJsonString
    } else if callee == "json_int" {
        CallKind::BuiltinJsonInt
    } else if callee == "http_get" {
        CallKind::BuiltinHttpGet
    } else if callee == "http_post" {
        CallKind::BuiltinHttpPost
    } else if struct_signatures.contains_key(callee) {
        CallKind::Struct
    } else if enum_signatures.contains_key(callee) {
        CallKind::Enum
    } else {
        CallKind::Unknown
    }
}

fn call_return_type(
    callee: &str,
    call_kind: CallKind,
    args: &[TypedExpr],
    signatures: &HashMap<String, FunctionSignature>,
    _struct_signatures: &HashMap<String, StructSignature>,
) -> Type {
    match call_kind {
        CallKind::BuiltinLen => Type::Int,
        CallKind::BuiltinPrint => Type::Unit,
        CallKind::BuiltinAppend => infer_append_return_type(args),
        CallKind::BuiltinContains => Type::Bool,
        CallKind::BuiltinFirst => infer_first_or_last_return_type(args),
        CallKind::BuiltinLast => infer_first_or_last_return_type(args),
        CallKind::BuiltinSlice => infer_slice_return_type(args),
        CallKind::BuiltinReverse => infer_reverse_return_type(args),
        CallKind::BuiltinSort => infer_sort_return_type(args),
        CallKind::BuiltinMin => infer_min_or_max_return_type(args),
        CallKind::BuiltinMax => infer_min_or_max_return_type(args),
        CallKind::BuiltinTrim => Type::String,
        CallKind::BuiltinSplit => infer_split_return_type(args),
        CallKind::BuiltinJoin => Type::String,
        CallKind::BuiltinStartsWith => Type::Bool,
        CallKind::BuiltinEndsWith => Type::Bool,
        CallKind::BuiltinParseInt => {
            Type::result(Type::Int, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinToString => Type::String,
        CallKind::BuiltinRange => Type::list(Type::Int),
        CallKind::BuiltinSleep => Type::Unit,
        CallKind::BuiltinAssert => Type::Unit,
        CallKind::BuiltinArgv => Type::list(Type::String),
        CallKind::BuiltinEnv => Type::result(Type::String, Type::Enum("RuntimeError".to_string())),
        CallKind::BuiltinCwd => Type::result(Type::String, Type::Enum("RuntimeError".to_string())),
        CallKind::BuiltinExists => Type::Bool,
        CallKind::BuiltinReadDir => Type::result(
            Type::list(Type::String),
            Type::Enum("RuntimeError".to_string()),
        ),
        CallKind::BuiltinMkdir | CallKind::BuiltinRemoveFile => {
            Type::result(Type::Unit, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinPathJoin
        | CallKind::BuiltinPathDir
        | CallKind::BuiltinPathBase
        | CallKind::BuiltinPathExt => Type::String,
        CallKind::BuiltinReadFile => {
            Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinWriteFile => {
            Type::result(Type::Unit, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinDict => Type::dict(Type::Unknown),
        CallKind::BuiltinInsert => infer_insert_return_type(args),
        CallKind::BuiltinKeys => infer_keys_return_type(args),
        CallKind::BuiltinValues => infer_values_return_type(args),
        CallKind::BuiltinChannel => Type::channel(Type::Unknown),
        CallKind::BuiltinClose => Type::Unit,
        CallKind::BuiltinSend => Type::result(Type::Unit, Type::Enum("RuntimeError".to_string())),
        CallKind::BuiltinRecv => Type::result(
            infer_recv_return_type(args),
            Type::Enum("RuntimeError".to_string()),
        ),
        CallKind::BuiltinCancelToken => Type::CancelToken,
        CallKind::BuiltinCancel => Type::Unit,
        CallKind::BuiltinIsCancelled => Type::Bool,
        CallKind::BuiltinTimeoutToken => Type::CancelToken,
        CallKind::BuiltinCancelAfter => Type::Unit,
        CallKind::BuiltinJsonParse => {
            Type::result(Type::Json, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinJsonStringify => {
            Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinJsonGet | CallKind::BuiltinJsonIndex => {
            Type::result(Type::Json, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinJsonLen | CallKind::BuiltinJsonInt => {
            Type::result(Type::Int, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinJsonString => {
            Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinHttpGet => {
            Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::BuiltinHttpPost => {
            Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
        }
        CallKind::Function => signatures
            .get(callee)
            .map(|signature| signature.return_type.clone())
            .unwrap_or(Type::Unknown),
        CallKind::Struct => Type::Struct(callee.to_string()),
        CallKind::Enum => Type::Unknown,
        CallKind::Unknown => Type::Unknown,
    }
}

fn validate_unary_expr(
    op: UnaryOp,
    value: &TypedExpr,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    match op {
        UnaryOp::Not if !matches!(value.ty, Type::Bool | Type::Unknown) => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3026",
                    "`not` requires a `bool` operand",
                    format!("this operand resolves to `{}`", value.ty.display_name()),
                    value.span,
                )
                .with_fix_it("apply `not` only to boolean expressions")
                .with_source_path(source_path.to_path_buf()),
            );
        }
        UnaryOp::Neg if !matches!(value.ty, Type::Int | Type::Unknown) => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3065",
                    "unary `-` requires an `int` operand",
                    format!("this operand resolves to `{}`", value.ty.display_name()),
                    value.span,
                )
                .with_fix_it("apply unary `-` only to integer expressions")
                .with_source_path(source_path.to_path_buf()),
            );
        }
        _ => {}
    }
}

fn infer_unary_type(op: UnaryOp, value: &Type) -> Type {
    match (op, value) {
        (UnaryOp::Not, Type::Bool) => Type::Bool,
        (UnaryOp::Not, Type::Unknown) => Type::Unknown,
        (UnaryOp::Neg, Type::Int) => Type::Int,
        (UnaryOp::Neg, Type::Unknown) => Type::Unknown,
        _ => Type::Unknown,
    }
}

fn validate_binary_expr(
    op: BinaryOp,
    lhs: &TypedExpr,
    rhs: &TypedExpr,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        if !matches!(lhs.ty, Type::Bool | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3025",
                    "logical operators require `bool` operands",
                    format!("the left operand resolves to `{}`", lhs.ty.display_name()),
                    lhs.span,
                )
                .with_fix_it("use `and` and `or` only with boolean expressions")
                .with_source_path(source_path.to_path_buf()),
            );
        }
        if !matches!(rhs.ty, Type::Bool | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3025",
                    "logical operators require `bool` operands",
                    format!("the right operand resolves to `{}`", rhs.ty.display_name()),
                    rhs.span,
                )
                .with_fix_it("use `and` and `or` only with boolean expressions")
                .with_source_path(source_path.to_path_buf()),
            );
        }
        return;
    }

    if matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
    ) {
        let ints_ok = matches!(lhs.ty, Type::Int | Type::Unknown)
            && matches!(rhs.ty, Type::Int | Type::Unknown);
        let string_add_ok = matches!(op, BinaryOp::Add)
            && matches!(lhs.ty, Type::String | Type::Unknown)
            && matches!(rhs.ty, Type::String | Type::Unknown);
        if !ints_ok && !string_add_ok {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3066",
                    format!("`{}` requires numeric operands", binary_op_name(op)),
                    format!(
                        "the operands resolve to `{}` and `{}`",
                        lhs.ty.display_name(),
                        rhs.ty.display_name()
                    ),
                    lhs.span,
                )
                .with_fix_it("use integer operands, or use `+` only for two strings")
                .with_source_path(source_path.to_path_buf()),
            );
        }
        return;
    }

    if is_equality_op(op)
        && !supports_equality(&lhs.ty, &rhs.ty, struct_signatures, enum_signatures)
    {
        diagnostics.push(
            Diagnostic::error(
                "GOF3087",
                format!(
                    "`{}` requires operands with explicit equality semantics",
                    binary_op_name(op)
                ),
                format!(
                    "the operands resolve to `{}` and `{}`",
                    lhs.ty.display_name(),
                    rhs.ty.display_name()
                ),
                lhs.span,
            )
            .with_fix_it(
                "compare matching int, string, bool, json, unit, or structural values built from comparable members",
            )
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if is_ordering_op(op) && !supports_ordering(&lhs.ty, &rhs.ty) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3087",
                format!("`{}` requires ordered operands", binary_op_name(op)),
                format!(
                    "the operands resolve to `{}` and `{}`",
                    lhs.ty.display_name(),
                    rhs.ty.display_name()
                ),
                lhs.span,
            )
            .with_fix_it("order only `int` or `string` values")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn infer_binary_type(
    lhs: &Type,
    op: BinaryOp,
    rhs: &Type,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
) -> Type {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
            match (lhs, op, rhs) {
                (
                    Type::Int,
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod,
                    Type::Int,
                ) => Type::Int,
                (Type::String, BinaryOp::Add, Type::String) => Type::String,
                _ => Type::Unknown,
            }
        }
        BinaryOp::And | BinaryOp::Or => match (lhs, rhs) {
            (Type::Bool, Type::Bool) => Type::Bool,
            _ => Type::Unknown,
        },
        BinaryOp::Eq | BinaryOp::Ne => {
            if supports_equality(lhs, rhs, struct_signatures, enum_signatures) {
                Type::Bool
            } else {
                Type::Unknown
            }
        }
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            if supports_ordering(lhs, rhs) {
                Type::Bool
            } else {
                Type::Unknown
            }
        }
    }
}

fn is_equality_op(op: BinaryOp) -> bool {
    matches!(op, BinaryOp::Eq | BinaryOp::Ne)
}

fn is_ordering_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
    )
}

fn supports_ordering(lhs: &Type, rhs: &Type) -> bool {
    matches!(
        (lhs, rhs),
        (Type::Unknown, _)
            | (_, Type::Unknown)
            | (Type::Int, Type::Int)
            | (Type::String, Type::String)
    )
}

fn supports_equality(
    lhs: &Type,
    rhs: &Type,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
) -> bool {
    if matches!(lhs, Type::Unknown) || matches!(rhs, Type::Unknown) {
        return true;
    }

    if !types_compatible(lhs, rhs) {
        return false;
    }

    type_supports_self_equality(lhs, struct_signatures, enum_signatures)
        && type_supports_self_equality(rhs, struct_signatures, enum_signatures)
}

fn type_supports_self_equality(
    ty: &Type,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
) -> bool {
    let mut active_structs = HashSet::new();
    let mut active_enums = HashSet::new();
    type_supports_self_equality_inner(
        ty,
        struct_signatures,
        enum_signatures,
        &mut active_structs,
        &mut active_enums,
    )
}

fn type_supports_self_equality_inner(
    ty: &Type,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
    active_structs: &mut HashSet<String>,
    active_enums: &mut HashSet<String>,
) -> bool {
    match ty {
        Type::Unknown | Type::Int | Type::String | Type::Bool | Type::Json | Type::Unit => true,
        Type::List(inner) | Type::Dict(inner) | Type::Channel(inner) | Type::Task(inner) => {
            match ty {
                Type::List(_) | Type::Dict(_) => type_supports_self_equality_inner(
                    inner,
                    struct_signatures,
                    enum_signatures,
                    active_structs,
                    active_enums,
                ),
                Type::Channel(_) | Type::Task(_) => false,
                _ => unreachable!("non-list/dict/channel/task branch should be unreachable"),
            }
        }
        Type::CancelToken => false,
        Type::Struct(name) => {
            if !active_structs.insert(name.clone()) {
                return true;
            }
            let result = struct_signatures.get(name).is_some_and(|signature| {
                signature.fields.iter().all(|field| {
                    type_supports_self_equality_inner(
                        &field.ty,
                        struct_signatures,
                        enum_signatures,
                        active_structs,
                        active_enums,
                    )
                })
            });
            active_structs.remove(name);
            result
        }
        Type::Enum(name) => {
            if !active_enums.insert(name.clone()) {
                return true;
            }
            let result = enum_signatures.get(name).is_some_and(|signature| {
                signature.variants.iter().all(|variant| {
                    variant.fields.iter().all(|field| {
                        type_supports_self_equality_inner(
                            &field.ty,
                            struct_signatures,
                            enum_signatures,
                            active_structs,
                            active_enums,
                        )
                    })
                })
            });
            active_enums.remove(name);
            result
        }
        Type::Result(ok, err) => {
            type_supports_self_equality_inner(
                ok,
                struct_signatures,
                enum_signatures,
                active_structs,
                active_enums,
            ) && type_supports_self_equality_inner(
                err,
                struct_signatures,
                enum_signatures,
                active_structs,
                active_enums,
            )
        }
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

fn infer_field_type(
    target: &TypedExpr,
    field: &str,
    diagnostics: &mut Diagnostics,
    span: Span,
    struct_signatures: &HashMap<String, StructSignature>,
    source_path: &Path,
) -> Type {
    match &target.ty {
        Type::Struct(name) => {
            let Some(signature) = struct_signatures.get(name) else {
                return Type::Unknown;
            };

            match signature
                .fields
                .iter()
                .find(|candidate| candidate.name == field)
            {
                Some(field_signature) => field_signature.ty.clone(),
                None => {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3022",
                            format!("unknown field `{field}` on `{name}`"),
                            "field access must reference a field declared on the target struct",
                            span,
                        )
                        .with_fix_it("use one of the fields declared on the struct")
                        .with_source_path(source_path.to_path_buf()),
                    );
                    Type::Unknown
                }
            }
        }
        Type::Unknown => Type::Unknown,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3024",
                    "field access requires a struct value",
                    format!("this target resolves to `{}`", other.display_name()),
                    span,
                )
                .with_fix_it("access fields only on struct values")
                .with_source_path(source_path.to_path_buf()),
            );
            Type::Unknown
        }
    }
}

fn resolve_enum_variant_reference(
    enum_name: &str,
    variant: &str,
    diagnostics: &mut Diagnostics,
    span: Span,
    enum_signatures: &HashMap<String, EnumSignature>,
    source_path: &Path,
) -> (Type, Vec<EnumVariantFieldSignature>) {
    let Some(signature) = enum_signatures.get(enum_name) else {
        return (Type::Unknown, Vec::new());
    };

    if let Some(variant_signature) = signature
        .variants
        .iter()
        .find(|candidate| candidate.name == variant)
    {
        (
            Type::Enum(enum_name.to_string()),
            variant_signature.fields.clone(),
        )
    } else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3028",
                format!("unknown variant `{variant}` on `{enum_name}`"),
                "enum variant references must use a variant declared on the enum",
                span,
            )
            .with_fix_it("use one of the variants declared on the enum")
            .with_source_path(source_path.to_path_buf()),
        );
        (Type::Unknown, Vec::new())
    }
}

fn resolve_enum_variant_call(
    enum_name: &str,
    variant: &str,
    args: &[TypedExpr],
    span: Span,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    let (ty, fields) = resolve_enum_variant_reference(
        enum_name,
        variant,
        diagnostics,
        span,
        enum_signatures,
        source_path,
    );
    if ty.is_unknown() {
        return ty;
    }

    if fields.len() != args.len() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3069",
                format!("wrong number of payload values for `{enum_name}.{variant}`"),
                format!(
                    "expected {} payload value(s), got {}",
                    fields.len(),
                    args.len()
                ),
                span,
            )
            .with_fix_it(format!(
                "pass {} payload value(s) to `{}.{}`",
                fields.len(),
                enum_name,
                variant
            ))
            .with_source_path(source_path.to_path_buf()),
        );
        return ty;
    }

    for (index, (expected, actual)) in fields.iter().zip(args.iter()).enumerate() {
        ensure_type_compatibility(
            &expected.ty,
            &actual.ty,
            actual.span,
            diagnostics,
            format!(
                "payload value {} for `{enum_name}.{variant}` has incompatible type",
                index + 1
            ),
            format!(
                "payload field `{}` expects `{}`, but the argument resolves to `{}`",
                expected.name,
                expected.ty.display_name(),
                actual.ty.display_name()
            ),
            source_path,
        );
    }

    ty
}

fn resolve_result_variant_reference(
    variant: &str,
    diagnostics: &mut Diagnostics,
    span: Span,
    source_path: &Path,
) -> Type {
    let note = match variant {
        "Ok" => "construct it as `Result.Ok(value)`",
        "Err" => "construct it as `Result.Err(error)`",
        _ => "",
    };

    if matches!(variant, "Ok" | "Err") {
        diagnostics.push(
            Diagnostic::error(
                "GOF3072",
                format!("result variant `Result.{variant}` requires a payload value"),
                "both `Result.Ok` and `Result.Err` carry exactly one payload value",
                span,
            )
            .with_fix_it(note)
            .with_source_path(source_path.to_path_buf()),
        );
    } else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3072",
                format!("unknown result variant `Result.{variant}`"),
                "the builtin result type exposes only `Result.Ok(value)` and `Result.Err(error)`",
                span,
            )
            .with_fix_it("use `Result.Ok(value)` or `Result.Err(error)`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    Type::Unknown
}

fn resolve_result_variant_call(
    variant: &str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    if !matches!(variant, "Ok" | "Err") {
        diagnostics.push(
            Diagnostic::error(
                "GOF3072",
                format!("unknown result variant `Result.{variant}`"),
                "the builtin result type exposes only `Result.Ok(value)` and `Result.Err(error)`",
                span,
            )
            .with_fix_it("use `Result.Ok(value)` or `Result.Err(error)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return Type::Unknown;
    }

    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3073",
                format!("wrong number of payload values for `Result.{variant}`"),
                format!("expected 1 payload value, got {}", args.len()),
                span,
            )
            .with_fix_it(format!(
                "pass exactly one payload value to `Result.{variant}`"
            ))
            .with_source_path(source_path.to_path_buf()),
        );
        return Type::Unknown;
    }

    match variant {
        "Ok" => Type::result(args[0].ty.clone(), Type::Unknown),
        "Err" => Type::result(Type::Unknown, args[0].ty.clone()),
        _ => Type::Unknown,
    }
}

fn validate_propagate_expr(
    value: &TypedExpr,
    function_return_type: &Type,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    let (ok_type, error_type) = match &value.ty {
        Type::Result(ok, error) => (ok.as_ref().clone(), error.as_ref().clone()),
        Type::Unknown => return Type::Unknown,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3074",
                    "postfix `?` requires a `Result[T, E]` operand",
                    format!("this operand resolves to `{}`", other.display_name()),
                    span,
                )
                .with_fix_it("apply `?` only to expressions that return `Result[T, E]`")
                .with_source_path(source_path.to_path_buf()),
            );
            return Type::Unknown;
        }
    };

    match function_return_type {
        Type::Result(_, function_error_type) => {
            ensure_type_compatibility(
                function_error_type,
                &error_type,
                span,
                diagnostics,
                "postfix `?` propagates an incompatible result error type".to_string(),
                format!(
                    "the operand can propagate `{}`, but this function currently returns `{}`",
                    error_type.display_name(),
                    function_return_type.display_name()
                ),
                source_path,
            );
        }
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3075",
                "postfix `?` requires a surrounding `Result` return contract",
                format!(
                    "this function currently resolves to `{}`, so it cannot propagate `Err(...)` with `?`",
                    other.display_name()
                ),
                span,
            )
            .with_fix_it("declare or infer the enclosing function as returning `Result[T, E]`")
            .with_source_path(source_path.to_path_buf()),
        ),
    }

    ok_type
}

fn infer_return_type(
    function_name: &str,
    stmts: &[TypedStmt],
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    let mut returns = ReturnAccumulator {
        ty: None,
        span: None,
        incompatible: false,
    };
    collect_return_types(function_name, stmts, &mut returns, diagnostics, source_path);
    returns.ty.unwrap_or(Type::Unit)
}

fn collect_return_types(
    function_name: &str,
    stmts: &[TypedStmt],
    returns: &mut ReturnAccumulator,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    for stmt in stmts {
        match stmt {
            TypedStmt::Return(expr) => {
                let candidate =
                    wrap_type_with_propagation(&expr.ty, expr_propagated_error_type(expr));
                merge_return_candidate(
                    function_name,
                    returns,
                    &candidate,
                    expr.span,
                    diagnostics,
                    source_path,
                );
            }
            TypedStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_return_types(function_name, then_body, returns, diagnostics, source_path);
                collect_return_types(function_name, else_body, returns, diagnostics, source_path);
            }
            TypedStmt::While { body, .. } => {
                collect_return_types(function_name, body, returns, diagnostics, source_path);
            }
            TypedStmt::For { body, .. } => {
                collect_return_types(function_name, body, returns, diagnostics, source_path);
            }
            TypedStmt::Match { arms, .. } => {
                for arm in arms {
                    collect_return_types(
                        function_name,
                        &arm.body,
                        returns,
                        diagnostics,
                        source_path,
                    );
                }
            }
            TypedStmt::Select { arms } => {
                for arm in arms {
                    collect_return_types(
                        function_name,
                        &arm.body,
                        returns,
                        diagnostics,
                        source_path,
                    );
                }
            }
            TypedStmt::Bind { .. }
            | TypedStmt::Assign { .. }
            | TypedStmt::Break
            | TypedStmt::Continue
            | TypedStmt::Expr(_) => {}
        }
    }
}

fn merge_return_candidate(
    function_name: &str,
    returns: &mut ReturnAccumulator,
    candidate: &Type,
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if returns.incompatible {
        return;
    }

    let Some(current) = returns.ty.clone() else {
        returns.ty = Some(candidate.clone());
        returns.span = Some(span);
        return;
    };

    match merge_return_types(&current, candidate) {
        Some(merged) => {
            let should_replace_anchor = current.is_unknown() && !candidate.is_unknown();
            returns.ty = Some(merged);
            if should_replace_anchor {
                returns.span = Some(span);
            }
        }
        None => {
            let anchor_span = returns.span.unwrap_or(span);
            diagnostics.push(
                Diagnostic::error(
                    "GOF3011",
                    format!("incompatible return types in `{function_name}`"),
                    format!(
                        "first return path resolves to `{}`, but this path resolves to `{}`",
                        current.display_name(),
                        candidate.display_name()
                    ),
                    span,
                )
                .with_fix_it(format!(
                    "make every `return` in `{function_name}` produce the same type (first return at {}:{})",
                    anchor_span.line, anchor_span.column
                ))
                .with_source_path(source_path.to_path_buf()),
            );
            returns.ty = Some(Type::Unknown);
            returns.incompatible = true;
        }
    }
}

fn wrap_type_with_propagation(base: &Type, propagated_error: Option<Type>) -> Type {
    let Some(propagated_error) = propagated_error else {
        return base.clone();
    };

    match base {
        Type::Result(ok, error) => Type::result(
            ok.as_ref().clone(),
            merge_return_types(error, &propagated_error).unwrap_or(propagated_error),
        ),
        _ => Type::result(base.clone(), propagated_error),
    }
}

fn expr_propagated_error_type(expr: &TypedExpr) -> Option<Type> {
    let direct = match &expr.kind {
        TypedExprKind::Propagate { value } => match &value.ty {
            Type::Result(_, error) => Some(error.as_ref().clone()),
            _ => Some(Type::Unknown),
        },
        _ => None,
    };

    merge_optional_error_type(
        direct,
        match &expr.kind {
            TypedExprKind::List { items } => items.iter().fold(None, |acc, item| {
                merge_optional_error_type(acc, expr_propagated_error_type(item))
            }),
            TypedExprKind::Dict { entries } => entries.iter().fold(None, |acc, entry| {
                let acc = merge_optional_error_type(acc, expr_propagated_error_type(&entry.key));
                merge_optional_error_type(acc, expr_propagated_error_type(&entry.value))
            }),
            TypedExprKind::Call { args, .. }
            | TypedExprKind::StructInit { args, .. }
            | TypedExprKind::EnumVariant { args, .. }
            | TypedExprKind::Spawn { args, .. } => args.iter().fold(None, |acc, arg| {
                merge_optional_error_type(acc, expr_propagated_error_type(arg))
            }),
            TypedExprKind::MethodCall { target, args, .. } => {
                let acc = expr_propagated_error_type(target);
                args.iter().fold(acc, |acc, arg| {
                    merge_optional_error_type(acc, expr_propagated_error_type(arg))
                })
            }
            TypedExprKind::Field { target, .. }
            | TypedExprKind::Await { value: target }
            | TypedExprKind::Unary { value: target, .. }
            | TypedExprKind::Propagate { value: target } => expr_propagated_error_type(target),
            TypedExprKind::Index { target, index } => merge_optional_error_type(
                expr_propagated_error_type(target),
                expr_propagated_error_type(index),
            ),
            TypedExprKind::Binary { lhs, rhs, .. } => merge_optional_error_type(
                expr_propagated_error_type(lhs),
                expr_propagated_error_type(rhs),
            ),
            TypedExprKind::Int(_)
            | TypedExprKind::String(_)
            | TypedExprKind::Bool(_)
            | TypedExprKind::Local(_) => None,
        },
    )
}

fn merge_optional_error_type(current: Option<Type>, candidate: Option<Type>) -> Option<Type> {
    match (current, candidate) {
        (None, None) => None,
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (Some(current), Some(candidate)) => {
            Some(merge_return_types(&current, &candidate).unwrap_or(Type::Unknown))
        }
    }
}

fn merge_return_types(current: &Type, candidate: &Type) -> Option<Type> {
    if current == candidate {
        return Some(current.clone());
    }

    if current.is_unknown() {
        return Some(candidate.clone());
    }

    if candidate.is_unknown() {
        return Some(current.clone());
    }

    match (current, candidate) {
        (Type::List(current), Type::List(candidate)) => {
            merge_return_types(current, candidate).map(Type::list)
        }
        (Type::Dict(current), Type::Dict(candidate)) => {
            merge_return_types(current, candidate).map(Type::dict)
        }
        (Type::Channel(current), Type::Channel(candidate)) => {
            merge_return_types(current, candidate).map(Type::channel)
        }
        (Type::Task(current), Type::Task(candidate)) => {
            merge_return_types(current, candidate).map(Type::task)
        }
        (Type::Result(current_ok, current_err), Type::Result(candidate_ok, candidate_err)) => {
            let ok = merge_return_types(current_ok, candidate_ok)?;
            let err = merge_return_types(current_err, candidate_err)?;
            Some(Type::result(ok, err))
        }
        _ => None,
    }
}

fn combine_declared_and_actual_type(declared: &Type, actual: &Type) -> Type {
    merge_return_types(declared, actual).unwrap_or_else(|| {
        if declared.is_unknown() {
            actual.clone()
        } else {
            declared.clone()
        }
    })
}

fn ensure_type_compatibility(
    expected: &Type,
    actual: &Type,
    span: Span,
    diagnostics: &mut Diagnostics,
    message: String,
    note: String,
    source_path: &Path,
) {
    if !types_compatible(expected, actual) {
        diagnostics.push(
            Diagnostic::error("GOF3013", message, note, span)
                .with_fix_it("align the declared type and the expression type")
                .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn types_compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Unknown, _) | (_, Type::Unknown) => true,
        (Type::List(expected), Type::List(actual)) => types_compatible(expected, actual),
        (Type::Dict(expected), Type::Dict(actual)) => types_compatible(expected, actual),
        (Type::Channel(expected), Type::Channel(actual)) => types_compatible(expected, actual),
        (Type::Task(expected), Type::Task(actual)) => types_compatible(expected, actual),
        (Type::Result(expected_ok, expected_err), Type::Result(actual_ok, actual_err)) => {
            types_compatible(expected_ok, actual_ok) && types_compatible(expected_err, actual_err)
        }
        (expected, actual) => expected == actual,
    }
}

fn infer_list_element_type(
    items: &[TypedExpr],
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    let mut element_type = Type::Unknown;

    for item in items {
        match merge_return_types(&element_type, &item.ty) {
            Some(merged) => element_type = merged,
            None => {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3017",
                        "list literal contains incompatible element types",
                        format!(
                            "the list started as `{}`, but this element resolves to `{}`",
                            element_type.display_name(),
                            item.ty.display_name()
                        ),
                        item.span,
                    )
                    .with_fix_it("make every element in the list resolve to one compatible type")
                    .with_source_path(source_path.to_path_buf()),
                );
                return Type::Unknown;
            }
        }
    }

    if items.is_empty() {
        Type::Unknown
    } else {
        element_type
    }
}

fn infer_dict_value_type(
    entries: &[TypedDictEntry],
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Type {
    let mut value_type = Type::Unknown;

    for entry in entries {
        if !matches!(entry.key.ty, Type::String | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3049",
                    "dict literal keys must resolve to `string`",
                    format!("this key resolves to `{}`", entry.key.ty.display_name()),
                    entry.key.span,
                )
                .with_fix_it("use a string key like `{\"name\": value}`")
                .with_source_path(source_path.to_path_buf()),
            );
        }

        match merge_return_types(&value_type, &entry.value.ty) {
            Some(merged) => value_type = merged,
            None => {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3050",
                        "dict literal contains incompatible value types",
                        format!(
                            "the dict started as `dict[{}]`, but this value resolves to `{}`",
                            value_type.display_name(),
                            entry.value.ty.display_name()
                        ),
                        entry.value.span,
                    )
                    .with_fix_it("make every dict value resolve to one compatible type")
                    .with_source_path(source_path.to_path_buf()),
                );
                return Type::Unknown;
            }
        }
    }

    value_type
}

fn infer_index_type(
    target: &TypedExpr,
    index: &TypedExpr,
    diagnostics: &mut Diagnostics,
    span: Span,
    source_path: &Path,
) -> Type {
    let mut valid = true;

    let element_type = match &target.ty {
        Type::List(inner) => {
            if !matches!(index.ty, Type::Int | Type::Unknown) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3018",
                        "list indexing requires an `int` index",
                        format!("this index resolves to `{}`", index.ty.display_name()),
                        index.span,
                    )
                    .with_fix_it("use an integer index like `values[0]`")
                    .with_source_path(source_path.to_path_buf()),
                );
                valid = false;
            }
            inner.as_ref().clone()
        }
        Type::Dict(inner) => {
            if !matches!(index.ty, Type::String | Type::Unknown) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3018",
                        "dict indexing requires a `string` key",
                        format!("this index resolves to `{}`", index.ty.display_name()),
                        index.span,
                    )
                    .with_fix_it("use a string key like `values[\"name\"]`")
                    .with_source_path(source_path.to_path_buf()),
                );
                valid = false;
            }
            inner.as_ref().clone()
        }
        Type::Unknown => Type::Unknown,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3018",
                    "indexing requires a list or dict value",
                    format!("this target resolves to `{}`", other.display_name()),
                    span,
                )
                .with_fix_it("index only list or dict values")
                .with_source_path(source_path.to_path_buf()),
            );
            valid = false;
            Type::Unknown
        }
    };

    if valid { element_type } else { Type::Unknown }
}

fn validate_len_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `len`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `len` with exactly one list or string argument")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    let arg = &args[0];
    if !matches!(
        arg.ty,
        Type::List(_) | Type::Dict(_) | Type::String | Type::Unknown
    ) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3019",
                "`len` requires a list, dict, or string value",
                format!("this argument resolves to `{}`", arg.ty.display_name()),
                arg.span,
            )
            .with_fix_it("pass a list, dict, or string value to `len`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_print_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `print`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `print` with exactly one printable value")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    let arg = &args[0];
    if !is_printable_type(&arg.ty) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3038",
                "`print` requires a printable value",
                format!("this argument resolves to `{}`", arg.ty.display_name()),
                arg.span,
            )
            .with_fix_it("print ints, strings, bools, lists, structs, or enums instead")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_append_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `append`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `append` as `append(list_value, item)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    let list_arg = &args[0];
    let value_arg = &args[1];
    match &list_arg.ty {
        Type::List(inner) => ensure_type_compatibility(
            inner,
            &value_arg.ty,
            value_arg.span,
            diagnostics,
            "appended value has an incompatible type".to_string(),
            format!(
                "the list stores `{}`, but the appended value resolves to `{}`",
                inner.display_name(),
                value_arg.ty.display_name()
            ),
            source_path,
        ),
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3039",
                "`append` requires a list as its first argument",
                format!("this argument resolves to `{}`", other.display_name()),
                list_arg.span,
            )
            .with_fix_it("pass a list value as the first argument to `append`")
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn validate_contains_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `contains`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `contains` as `contains(haystack, needle)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    let haystack = &args[0];
    let needle = &args[1];
    match &haystack.ty {
        Type::String => {
            if !matches!(needle.ty, Type::String | Type::Unknown) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3040",
                        "`contains` requires a string needle for string haystacks",
                        format!("this needle resolves to `{}`", needle.ty.display_name()),
                        needle.span,
                    )
                    .with_fix_it("pass a string as the second argument to `contains`")
                    .with_source_path(source_path.to_path_buf()),
                );
            }
        }
        Type::List(inner) => ensure_type_compatibility(
            inner,
            &needle.ty,
            needle.span,
            diagnostics,
            "needle has an incompatible type for `contains`".to_string(),
            format!(
                "the list stores `{}`, but the needle resolves to `{}`",
                inner.display_name(),
                needle.ty.display_name()
            ),
            source_path,
        ),
        Type::Dict(_) => {
            if !matches!(needle.ty, Type::String | Type::Unknown) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3040",
                        "`contains` requires a string key for dict haystacks",
                        format!("this needle resolves to `{}`", needle.ty.display_name()),
                        needle.span,
                    )
                    .with_fix_it("pass a string key as the second argument to `contains`")
                    .with_source_path(source_path.to_path_buf()),
                );
            }
        }
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3040",
                "`contains` requires a string, list, or dict haystack",
                format!("this haystack resolves to `{}`", other.display_name()),
                haystack.span,
            )
            .with_fix_it("call `contains` with a string, list, or dict as the first argument")
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn validate_first_or_last_call(
    callee: &'static str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{callee}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{callee}(list_value)`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::List(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3088",
                format!("`{callee}` requires a list value"),
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it(format!("pass a list value to `{callee}`"))
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_slice_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 3 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `slice`",
                format!("expected 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `slice(list_value, start, end)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::List(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3088",
                "`slice` requires a list as its first argument",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a list value as the first argument to `slice`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    for (index, arg) in args[1..].iter().enumerate() {
        if !matches!(arg.ty, Type::Int | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3088",
                    format!("`slice` argument {} must resolve to `int`", index + 2),
                    format!("this argument resolves to `{}`", arg.ty.display_name()),
                    arg.span,
                )
                .with_fix_it("pass integer start and end indexes to `slice`")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }
}

fn validate_reverse_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `reverse`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `reverse(list_value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::List(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3088",
                "`reverse` requires a list value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a list value to `reverse`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_sort_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_orderable_list_call("sort", args, span, diagnostics, source_path);
}

fn validate_min_or_max_call(
    callee: &'static str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_orderable_list_call(callee, args, span, diagnostics, source_path);
}

fn validate_orderable_list_call(
    callee: &'static str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{callee}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{callee}(list_value)`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    match &args[0].ty {
        Type::List(inner) if matches!(inner.as_ref(), Type::Int | Type::String | Type::Unknown) => {
        }
        Type::List(inner) => diagnostics.push(
            Diagnostic::error(
                "GOF3088",
                format!("`{callee}` currently requires `list[int]` or `list[string]`"),
                format!("this list stores `{}`", inner.display_name()),
                args[0].span,
            )
            .with_fix_it(format!("call `{callee}` on a list of ints or strings"))
            .with_source_path(source_path.to_path_buf()),
        ),
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3088",
                format!("`{callee}` requires a list value"),
                format!("this argument resolves to `{}`", other.display_name()),
                args[0].span,
            )
            .with_fix_it(format!("pass a list value to `{callee}`"))
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn validate_trim_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `trim`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `trim(text)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3057",
                "`trim` requires a string value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string value like `trim(\"  gof  \")`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_split_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `split`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `split(text, separator)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3055",
                "`split` requires a string value as its first argument",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string value as the first argument to `split`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3055",
                "`split` requires a string separator",
                format!("this separator resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a string separator like `\",\"` to `split`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if let TypedExprKind::String(separator) = &args[1].kind {
        if separator.is_empty() {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3055",
                    "`split` requires a non-empty separator",
                    "an empty separator would make the bootstrap string contract ambiguous",
                    args[1].span,
                )
                .with_fix_it("pass a visible separator such as `\",\"` or `\"-\"`")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }
}

fn validate_join_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `join`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `join(parts, separator)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    match &args[0].ty {
        Type::List(inner) => {
            if !matches!(inner.as_ref(), Type::String | Type::Unknown) {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3056",
                        "`join` requires a list of strings as its first argument",
                        format!("this list stores `{}`", inner.display_name()),
                        args[0].span,
                    )
                    .with_fix_it("pass a `list[string]` value to `join`")
                    .with_source_path(source_path.to_path_buf()),
                );
            }
        }
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3056",
                "`join` requires a list of strings as its first argument",
                format!("this argument resolves to `{}`", other.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a `list[string]` value to `join`")
            .with_source_path(source_path.to_path_buf()),
        ),
    }

    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3056",
                "`join` requires a string separator",
                format!("this separator resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a string separator as the second argument to `join`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_starts_with_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_string_pair_call(
        args,
        span,
        diagnostics,
        source_path,
        "starts_with",
        "GOF3058",
        "call `starts_with(text, prefix)`",
    );
}

fn validate_ends_with_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_string_pair_call(
        args,
        span,
        diagnostics,
        source_path,
        "ends_with",
        "GOF3059",
        "call `ends_with(text, suffix)`",
    );
}

fn validate_parse_int_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `parse_int`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `parse_int(text)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3060",
                "`parse_int` requires a string value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string like `\"42\"` or `trim(text)` to `parse_int`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_to_string_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `to_string`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `to_string(value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !is_printable_type(&args[0].ty) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3062",
                "`to_string` requires a printable value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it(
                "pass an int, string, bool, list, dict, struct, or enum value to `to_string`",
            )
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_range_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !(1..=3).contains(&args.len()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `range`",
                format!("expected 1, 2, or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `range(stop)`, `range(start, stop)`, or `range(start, stop, step)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    for (index, arg) in args.iter().enumerate() {
        if !matches!(arg.ty, Type::Int | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3063",
                    format!("`range` argument {} must resolve to `int`", index + 1),
                    format!("this argument resolves to `{}`", arg.ty.display_name()),
                    arg.span,
                )
                .with_fix_it("pass integer values to `range`")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }

    if args.len() == 3 {
        if let TypedExprKind::Int(0) = args[2].kind {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3064",
                    "`range` step cannot be zero",
                    "a zero step would never make forward progress",
                    args[2].span,
                )
                .with_fix_it("pass a non-zero integer step to `range`")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }
}

fn validate_string_pair_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
    callee: &'static str,
    code: &'static str,
    fix_it: &str,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{callee}`"),
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it(fix_it)
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                code,
                format!("`{callee}` requires a string value as its first argument"),
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it(format!(
                "pass a string value as the first argument to `{callee}`"
            ))
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                code,
                format!("`{callee}` requires a string value as its second argument"),
                format!("this argument resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it(format!(
                "pass a string value as the second argument to `{callee}`"
            ))
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_assert_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !(1..=2).contains(&args.len()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `assert`",
                format!("expected 1 or 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `assert(condition)` or `assert(condition, \"message\")`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Bool | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3041",
                "`assert` requires a boolean condition",
                format!("this condition resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a boolean expression as the first argument to `assert`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if args.len() == 2 && !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3041",
                "`assert` requires a string message when a second argument is present",
                format!("this message resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a string as the second argument to `assert`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_no_argument_call(
    name: &str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !args.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{name}`"),
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{name}()` without arguments"))
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_single_string_argument_call(
    name: &str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{name}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{name}(path_or_text)`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3076",
                format!("`{name}` requires a string argument"),
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string value")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_single_int_argument_call(
    name: &str,
    code: &'static str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{name}`"),
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{name}(value)`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Int | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                code,
                format!("`{name}` requires an integer argument"),
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass an integer value")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_two_string_argument_call(
    name: &str,
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                format!("wrong number of arguments for `{name}`"),
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it(format!("call `{name}(left, right)`"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    for (index, arg) in args.iter().enumerate() {
        if !matches!(arg.ty, Type::String | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3076",
                    format!("`{name}` requires string arguments"),
                    format!(
                        "argument {} resolves to `{}`",
                        index + 1,
                        arg.ty.display_name()
                    ),
                    arg.span,
                )
                .with_fix_it("pass string values for both arguments")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }
}

fn validate_env_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_single_string_argument_call("env", args, span, diagnostics, source_path);
}

fn validate_sleep_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_single_int_argument_call("sleep", "GOF3085", args, span, diagnostics, source_path);

    if args.len() == 1 {
        validate_non_negative_duration_argument(
            "sleep",
            "GOF3086",
            &args[0],
            diagnostics,
            source_path,
        );
    }
}

fn validate_non_negative_duration_argument(
    builtin_name: &str,
    diagnostic_code: &'static str,
    arg: &TypedExpr,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    let value = match &arg.kind {
        TypedExprKind::Int(value) => Some(*value),
        TypedExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => match &value.kind {
            TypedExprKind::Int(value) => Some(-*value),
            _ => None,
        },
        _ => None,
    };

    let Some(value) = value else {
        return;
    };

    if value < 0 {
        diagnostics.push(
            Diagnostic::error(
                diagnostic_code,
                format!("`{builtin_name}` requires a non-negative duration"),
                format!("this duration resolves to `{value}`"),
                arg.span,
            )
            .with_fix_it("pass `0` or another non-negative millisecond count")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_close_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `close`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `close(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Channel(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3081",
                "`close` requires a channel value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a channel value to `close`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_cancel_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `cancel`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cancel(token)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::CancelToken | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3082",
                "`cancel` requires a cancellation token",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value created by `cancel_token()`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_is_cancelled_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `is_cancelled`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `is_cancelled(token)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::CancelToken | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3082",
                "`is_cancelled` requires a cancellation token",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value created by `cancel_token()`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_timeout_token_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    validate_single_int_argument_call(
        "timeout_token",
        "GOF3093",
        args,
        span,
        diagnostics,
        source_path,
    );

    if args.len() == 1 {
        validate_non_negative_duration_argument(
            "timeout_token",
            "GOF3094",
            &args[0],
            diagnostics,
            source_path,
        );
    }
}

fn validate_cancel_after_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `cancel_after`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `cancel_after(token, milliseconds)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::CancelToken | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3082",
                "`cancel_after` requires a cancellation token as its first argument",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value created by `cancel_token()`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if !matches!(args[1].ty, Type::Int | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3093",
                "`cancel_after` requires an integer duration",
                format!("this argument resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a non-negative millisecond count as the second argument")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    validate_non_negative_duration_argument(
        "cancel_after",
        "GOF3094",
        &args[1],
        diagnostics,
        source_path,
    );
}

fn validate_json_stringify_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_stringify`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_stringify(value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_stringify` requires a `json` value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value returned from `json_parse(...)` or another json helper")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_json_get_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_get`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_get(value, \"key\")`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_get` requires a `json` value as its first argument",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value returned from `json_parse(...)` or `json_get(...)`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3076",
                "`json_get` requires a string key",
                format!("this key resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a string key")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_json_index_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_index`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_index(value, index)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_index` requires a `json` value as its first argument",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a value returned from `json_parse(...)` or `json_get(...)`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
    if !matches!(args[1].ty, Type::Int | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3084",
                "`json_index` requires an integer index",
                format!("this index resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass an integer index like `0`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_json_len_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_len`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_len(value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }
    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_len` requires a `json` value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_json_string_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_string`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_string(value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }
    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_string` requires a `json` value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_json_int_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `json_int`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `json_int(value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }
    if !matches!(args[0].ty, Type::Json | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3083",
                "`json_int` requires a `json` value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a `json` value")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_http_post_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !(2..=3).contains(&args.len()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `http_post`",
                format!("expected 2 or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `http_post(url, body)` or `http_post(url, body, content_type)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    for (index, arg) in args.iter().enumerate() {
        if !matches!(arg.ty, Type::String | Type::Unknown) {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3076",
                    "`http_post` requires string arguments",
                    format!(
                        "argument {} resolves to `{}`",
                        index + 1,
                        arg.ty.display_name()
                    ),
                    arg.span,
                )
                .with_fix_it("pass string values for the URL, body, and optional content type")
                .with_source_path(source_path.to_path_buf()),
            );
        }
    }
}

fn validate_read_file_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `read_file`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `read_file(path)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3043",
                "`read_file` requires a string path",
                format!("this path resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string path like `\"notes.txt\"` to `read_file`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_write_file_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `write_file`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `write_file(path, contents)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3043",
                "`write_file` requires a string path",
                format!("this path resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a string path as the first argument to `write_file`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3043",
                "`write_file` requires string contents",
                format!(
                    "this contents value resolves to `{}`",
                    args[1].ty.display_name()
                ),
                args[1].span,
            )
            .with_fix_it("pass a string as the second argument to `write_file`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_dict_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !args.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `dict`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `dict()` without arguments")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_insert_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 3 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `insert`",
                format!("expected 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `insert(dict_value, \"key\", value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    match &args[0].ty {
        Type::Dict(inner) => ensure_type_compatibility(
            inner,
            &args[2].ty,
            args[2].span,
            diagnostics,
            "inserted value has an incompatible type".to_string(),
            format!(
                "the dict stores `{}`, but the inserted value resolves to `{}`",
                inner.display_name(),
                args[2].ty.display_name()
            ),
            source_path,
        ),
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3045",
                "`insert` requires a dict as its first argument",
                format!("this argument resolves to `{}`", other.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a dict value as the first argument to `insert`")
            .with_source_path(source_path.to_path_buf()),
        ),
    }

    if !matches!(args[1].ty, Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3045",
                "`insert` requires a string key",
                format!("this key resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a string as the second argument to `insert`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_keys_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `keys`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `keys(dict_value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Dict(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3051",
                "`keys` requires a dict value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a dict value like `keys(metrics)`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_values_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `values`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `values(dict_value)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Dict(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3052",
                "`values` requires a dict value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a dict value like `values(metrics)`")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_channel_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() > 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `channel`",
                format!("expected 0 or 1 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `channel()` or `channel(capacity)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    let Some(capacity) = args.first() else {
        return;
    };

    if !matches!(capacity.ty, Type::Int | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3096",
                "`channel` requires an integer capacity when an argument is provided",
                format!("this argument resolves to `{}`", capacity.ty.display_name()),
                capacity.span,
            )
            .with_fix_it("pass an integer channel capacity like `channel(0)` or `channel(2)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    validate_non_negative_channel_capacity(capacity, diagnostics, source_path);
}

fn validate_non_negative_channel_capacity(
    arg: &TypedExpr,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    let value = match &arg.kind {
        TypedExprKind::Int(value) => Some(*value),
        TypedExprKind::Unary {
            op: UnaryOp::Neg,
            value,
        } => match &value.kind {
            TypedExprKind::Int(value) => Some(-*value),
            _ => None,
        },
        _ => None,
    };

    let Some(value) = value else {
        return;
    };

    if value < 0 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3097",
                "`channel` requires a non-negative capacity",
                format!("this capacity resolves to `{value}`"),
                arg.span,
            )
            .with_fix_it("pass `0` or another non-negative channel capacity")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_send_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !(2..=3).contains(&args.len()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `send`",
                format!("expected 2 or 3 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `send(channel_value, item)` or `send(channel_value, item, token)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    match &args[0].ty {
        Type::Channel(inner) => ensure_type_compatibility(
            inner,
            &args[1].ty,
            args[1].span,
            diagnostics,
            "sent value has an incompatible channel type".to_string(),
            format!(
                "the channel carries `{}`, but the sent value resolves to `{}`",
                inner.display_name(),
                args[1].ty.display_name()
            ),
            source_path,
        ),
        Type::Unknown => {}
        other => diagnostics.push(
            Diagnostic::error(
                "GOF3046",
                "`send` requires a channel as its first argument",
                format!("this argument resolves to `{}`", other.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a channel value as the first argument to `send`")
            .with_source_path(source_path.to_path_buf()),
        ),
    }

    if args.len() == 3 && !matches!(args[2].ty, Type::CancelToken | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3082",
                "`send` requires a cancellation token as its optional third argument",
                format!("this argument resolves to `{}`", args[2].ty.display_name()),
                args[2].span,
            )
            .with_fix_it("pass a value created by `cancel_token()` as the third argument")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn validate_recv_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !(1..=2).contains(&args.len()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `recv`",
                format!("expected 1 or 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `recv(channel_value)` or `recv(channel_value, token)`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !matches!(args[0].ty, Type::Channel(_) | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3046",
                "`recv` requires a channel value",
                format!("this argument resolves to `{}`", args[0].ty.display_name()),
                args[0].span,
            )
            .with_fix_it("pass a channel value to `recv`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    if args.len() == 2 && !matches!(args[1].ty, Type::CancelToken | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3082",
                "`recv` requires a cancellation token as its optional second argument",
                format!("this argument resolves to `{}`", args[1].ty.display_name()),
                args[1].span,
            )
            .with_fix_it("pass a value created by `cancel_token()` as the second argument")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn infer_append_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 2 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) => merge_return_types(inner, &args[1].ty)
            .map(Type::list)
            .unwrap_or_else(|| Type::list(inner.as_ref().clone())),
        Type::Unknown => Type::Unknown,
        _ => Type::Unknown,
    }
}

fn infer_first_or_last_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) => Type::result(
            inner.as_ref().clone(),
            Type::Enum("RuntimeError".to_string()),
        ),
        Type::Unknown => Type::result(Type::Unknown, Type::Enum("RuntimeError".to_string())),
        _ => Type::Unknown,
    }
}

fn infer_slice_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 3 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) => Type::result(
            Type::list(inner.as_ref().clone()),
            Type::Enum("RuntimeError".to_string()),
        ),
        Type::Unknown => Type::result(
            Type::list(Type::Unknown),
            Type::Enum("RuntimeError".to_string()),
        ),
        _ => Type::Unknown,
    }
}

fn infer_reverse_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) => Type::list(inner.as_ref().clone()),
        Type::Unknown => Type::list(Type::Unknown),
        _ => Type::Unknown,
    }
}

fn infer_sort_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) if matches!(inner.as_ref(), Type::Int | Type::String | Type::Unknown) => {
            Type::list(inner.as_ref().clone())
        }
        Type::Unknown => Type::list(Type::Unknown),
        _ => Type::Unknown,
    }
}

fn infer_min_or_max_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::List(inner) if matches!(inner.as_ref(), Type::Int | Type::String | Type::Unknown) => {
            Type::result(
                inner.as_ref().clone(),
                Type::Enum("RuntimeError".to_string()),
            )
        }
        Type::Unknown => Type::result(Type::Unknown, Type::Enum("RuntimeError".to_string())),
        _ => Type::Unknown,
    }
}

fn infer_split_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 2 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::String | Type::Unknown => Type::list(Type::String),
        _ => Type::Unknown,
    }
}

fn infer_insert_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 3 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::Dict(inner) => merge_return_types(inner, &args[2].ty)
            .map(Type::dict)
            .unwrap_or_else(|| Type::dict(inner.as_ref().clone())),
        Type::Unknown => Type::dict(args[2].ty.clone()),
        _ => Type::Unknown,
    }
}

fn infer_keys_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::Dict(_) | Type::Unknown => Type::list(Type::String),
        _ => Type::Unknown,
    }
}

fn infer_values_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::Dict(inner) => Type::list(inner.as_ref().clone()),
        Type::Unknown => Type::list(Type::Unknown),
        _ => Type::Unknown,
    }
}

fn infer_recv_return_type(args: &[TypedExpr]) -> Type {
    if !(1..=2).contains(&args.len()) {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::Channel(inner) => inner.as_ref().clone(),
        _ => Type::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectOperationKind {
    Recv,
    Send,
}

fn validate_select_operation(
    operation: &TypedExpr,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> Option<SelectOperationKind> {
    match &operation.kind {
        TypedExprKind::Call { callee, args }
            if callee == "recv" && (1..=2).contains(&args.len()) =>
        {
            Some(SelectOperationKind::Recv)
        }
        TypedExprKind::Call { callee, args }
            if callee == "send" && (2..=3).contains(&args.len()) =>
        {
            Some(SelectOperationKind::Send)
        }
        TypedExprKind::Call { callee, .. } => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3047",
                    "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                    format!("this arm uses `{callee}(...)` instead"),
                    operation.span,
                )
                .with_fix_it(
                    "replace the arm operation with `recv(...)`, `send(...)`, or `default`",
                )
                .with_source_path(source_path.to_path_buf()),
            );
            None
        }
        _ => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3047",
                    "`select` arms currently require `recv(...)`, `send(...)`, or `default`",
                    "select arms must be written as `recv(channel):`, `recv(channel, token):`, `send(channel, value):`, `send(channel, value, token):`, `value = recv(channel):`, `value = recv(channel, token):`, `value = send(channel, value):`, `value = send(channel, value, token):`, or `default:`",
                    operation.span,
                )
                .with_fix_it("replace this arm with `recv(...)`, `send(...)`, or `default`")
                .with_source_path(source_path.to_path_buf()),
            );
            None
        }
    }
}

fn is_printable_type(ty: &Type) -> bool {
    match ty {
        Type::Int
        | Type::String
        | Type::Bool
        | Type::Json
        | Type::Struct(_)
        | Type::Enum(_)
        | Type::Unknown => true,
        Type::List(inner) | Type::Dict(inner) => !matches!(
            inner.as_ref(),
            Type::Task(_) | Type::Unit | Type::Channel(_) | Type::CancelToken
        ),
        Type::Result(ok, err) => is_printable_type(ok) && is_printable_type(err),
        Type::Channel(_) | Type::Task(_) | Type::CancelToken | Type::Unit => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Type, TypedExprKind, TypedMatchPattern, TypedSelectArmKind, TypedStmt, lower};
    use crate::ast::BinaryOp;
    use crate::ast::parse;
    use crate::cst::CstModule;
    use crate::hir::lower as lower_hir;
    use crate::lexer::lex;
    use crate::source::SourceFile;

    fn lower_source(text: &str) -> Result<super::TypedModule, crate::diagnostics::Diagnostics> {
        let source = SourceFile::new("typed.gof", text);
        let tokens = lex(&source).expect("lexing should succeed");
        let ast = parse(&CstModule::new(tokens)).expect("parsing should succeed");
        let hir = lower_hir(&ast);
        lower(&hir)
    }

    #[test]
    fn parameter_annotations_drive_return_and_task_types() {
        let module = lower_source(
            "fn square(x: int) -> int:\n    return x * x\nfn main() -> int:\n    task = go square(12)\n    return await task\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.functions[0].params[0].ty, Type::Int);
        assert_eq!(module.functions[0].return_type, Type::Int);
        assert_eq!(module.functions[1].return_type, Type::Int);

        match &module.functions[1].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Task(Box::new(Type::Int)));
            }
            other => panic!("expected first statement to be bind, got {other:?}"),
        }

        match &module.functions[1].body[1] {
            TypedStmt::Return(expr) => match &expr.kind {
                TypedExprKind::Await { .. } => assert_eq!(expr.ty, Type::Int),
                other => panic!("expected await return, got {other:?}"),
            },
            other => panic!("expected second statement to be return, got {other:?}"),
        }
    }

    #[test]
    fn infers_function_return_types_through_module_calls() {
        let module = lower_source(
            "fn truth() -> bool:\n    return true\nfn relay() -> bool:\n    return truth()\nfn main() -> int:\n    task = go relay()\n    if await task:\n        return 1\n    else:\n        return 0\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.functions[0].return_type, Type::Bool);
        assert_eq!(module.functions[1].return_type, Type::Bool);
        assert_eq!(module.functions[2].return_type, Type::Int);
    }

    #[test]
    fn builtin_runtime_error_task_variants_typecheck() {
        let module = lower_source(
            "fn choose(flag: bool) -> RuntimeError:\n    if flag:\n        return RuntimeError.TaskPanicked(\"worker\")\n    return RuntimeError.TaskFailed(\"boom\")\n",
        )
        .expect("task runtime error variants should typecheck");

        assert_eq!(
            module.functions[0].return_type,
            Type::Enum("RuntimeError".to_string())
        );
    }

    #[test]
    fn rejects_incompatible_function_returns() {
        let diagnostics = lower_source(
            "fn weird():\n    if true:\n        return 1\n    else:\n        return \"oops\"\nfn main():\n    return weird()\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3011"]);
    }

    #[test]
    fn rejects_unknown_type_annotations() {
        let diagnostics =
            lower_source("fn main(x: number) -> number:\n    value: number = 1\n    return x\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3012", "GOF3012", "GOF3012"]);
    }

    #[test]
    fn rejects_annotated_type_mismatches() {
        let diagnostics = lower_source(
            "fn add_one(x: int):\n    return x + 1\nfn main():\n    value: string = add_one(1)\n    return value\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3013"]);
    }

    #[test]
    fn respects_declared_return_types() {
        let module = lower_source(
            "fn truth() -> bool:\n    return true\nfn main() -> bool:\n    return truth()\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.functions[0].return_type, Type::Bool);
        assert_eq!(module.functions[1].return_type, Type::Bool);
    }

    #[test]
    fn rejects_declared_return_type_mismatches() {
        let diagnostics = lower_source(
            "fn truth() -> bool:\n    return 1\nfn main() -> bool:\n    return truth()\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3013"]);
    }

    #[test]
    fn infers_list_and_index_types() {
        let module = lower_source(
            "fn main() -> int:\n    values: list = [1, 2, 3]\n    return values[1] + len(values)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected list bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Return(expr) => assert_eq!(expr.ty, Type::Int),
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn rejects_incompatible_list_literals() {
        let diagnostics =
            lower_source("fn main() -> list:\n    values = [1, \"oops\"]\n    return values\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3017"]);
    }

    #[test]
    fn rejects_invalid_indexing() {
        let diagnostics = lower_source("fn main() -> int:\n    value = 42\n    return value[0]\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3018"]);
    }

    #[test]
    fn supports_dict_literals() {
        let module = lower_source(
            "fn main() -> int:\n    values: dict = {\"ok\": 2, \"warn\": 3}\n    return values[\"ok\"] + len(values)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Dict(Box::new(Type::Int)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Dict { entries } if entries.len() == 2)
                );
            }
            other => panic!("expected dict bind, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_dict_literal_keys() {
        let diagnostics = lower_source("fn main() -> dict:\n    return {1: 2}\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3049"]);
    }

    #[test]
    fn rejects_incompatible_dict_literal_values() {
        let diagnostics =
            lower_source("fn main() -> dict:\n    return {\"ok\": 1, \"bad\": \"oops\"}\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3050"]);
    }

    #[test]
    fn rejects_invalid_len_operand() {
        let diagnostics = lower_source("fn main() -> int:\n    return len(true)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3019"]);
    }

    #[test]
    fn supports_append_and_contains_builtins() {
        let module = lower_source(
            "fn main() -> bool:\n    values = append([1, 2], 3)\n    return contains(values, 3) and contains(\"gof-lang\", \"lang\")\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "append")
                );
            }
            other => panic!("expected append bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Return(expr) => assert_eq!(expr.ty, Type::Bool),
            other => panic!("expected bool return, got {other:?}"),
        }
    }

    #[test]
    fn supports_string_helper_builtins() {
        let module = lower_source(
            "fn main() -> int:\n    line = trim(\"  gof,lang  \")\n    parts = split(line, \",\")\n    merged = join(parts, \"-\")\n    assert(starts_with(merged, \"gof\"), \"expected prefix\")\n    assert(ends_with(merged, \"lang\"), \"expected suffix\")\n    return len(merged) + len(parts)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::String)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "split")
                );
            }
            other => panic!("expected split bind, got {other:?}"),
        }

        match &module.functions[0].body[2] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::String);
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "join")
                );
            }
            other => panic!("expected join bind, got {other:?}"),
        }
    }

    #[test]
    fn supports_conversion_builtins() {
        let module = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    raw = parse_int(trim(\" 41 \"))\n    parsed = raw?\n    rendered = \"gof-\" + to_string(parsed + 1)\n    assert(rendered == \"gof-42\", \"expected converted text\")\n    return Result.Ok(parsed + len(rendered))\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(
                    value.ty,
                    Type::result(Type::Int, Type::Enum("RuntimeError".to_string()))
                );
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "parse_int")
                );
            }
            other => panic!("expected parse_int bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Int);
                assert!(matches!(&value.kind, TypedExprKind::Propagate { .. }));
            }
            other => panic!("expected propagated parse_int bind, got {other:?}"),
        }

        match &module.functions[0].body[2] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::String);
            }
            other => panic!("expected rendered bind, got {other:?}"),
        }
    }

    #[test]
    fn supports_sequence_helper_builtins() {
        let module = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    values = [7, 1, 5, 3]\n    head = first(values)?\n    tail = last(values)?\n    middle = slice(values, 1, 3)?\n    reversed = reverse(values)\n    ordered = sort(values)\n    smallest = min(values)?\n    loudest = max([\"warn\", \"critical\", \"ok\"])?\n    return Result.Ok(head + tail + len(middle) + len(reversed) + len(ordered) + smallest + len(loudest))\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::Bind { value, .. } => assert_eq!(value.ty, Type::Int),
            other => panic!("expected propagated first bind, got {other:?}"),
        }
        match &module.functions[0].body[3] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected propagated slice bind, got {other:?}"),
        }
        match &module.functions[0].body[4] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected reverse bind, got {other:?}"),
        }
        match &module.functions[0].body[5] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected sort bind, got {other:?}"),
        }
        match &module.functions[0].body[6] {
            TypedStmt::Bind { value, .. } => assert_eq!(value.ty, Type::Int),
            other => panic!("expected propagated min bind, got {other:?}"),
        }
        match &module.functions[0].body[7] {
            TypedStmt::Bind { value, .. } => assert_eq!(value.ty, Type::String),
            other => panic!("expected propagated max bind, got {other:?}"),
        }
    }

    #[test]
    fn supports_sleep_and_http_post_builtins() {
        let module = lower_source(
            "fn main() -> Result[string, RuntimeError]:\n    sleep(0)\n    return http_post(\"https://example.invalid/send\", \"{}\", \"application/json\")\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Expr(expr) => {
                assert_eq!(expr.ty, Type::Unit);
                assert!(
                    matches!(&expr.kind, TypedExprKind::Call { callee, .. } if callee == "sleep")
                );
            }
            other => panic!("expected sleep expression, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Return(value) => {
                assert_eq!(
                    value.ty,
                    Type::result(Type::String, Type::Enum("RuntimeError".to_string()))
                );
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "http_post")
                );
            }
            other => panic!("expected http_post return, got {other:?}"),
        }
    }

    #[test]
    fn supports_timeout_token_and_cancel_after_builtins() {
        let module = lower_source(
            "fn main() -> bool:\n    token = timeout_token(25)\n    cancel_after(token, 0)\n    return is_cancelled(token)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::CancelToken);
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "timeout_token")
                );
            }
            other => panic!("expected timeout_token bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Expr(expr) => {
                assert_eq!(expr.ty, Type::Unit);
                assert!(
                    matches!(&expr.kind, TypedExprKind::Call { callee, .. } if callee == "cancel_after")
                );
            }
            other => panic!("expected cancel_after expression, got {other:?}"),
        }
    }

    #[test]
    fn supports_parameterized_builtin_type_annotations() {
        let module = lower_source(
            "fn first(values: list[int]) -> int:\n    return values[0]\nfn main() -> Result[dict[int], RuntimeError]:\n    ch: channel[int] = channel()\n    send(ch, first([7, 9]))?\n    return Result.Ok({\"ok\": recv(ch)?})\n",
        )
        .expect("typing should succeed");

        assert_eq!(
            module.functions[0].params[0].ty,
            Type::List(Box::new(Type::Int))
        );
        assert_eq!(
            module.functions[1].return_type,
            Type::Result(
                Box::new(Type::Dict(Box::new(Type::Int))),
                Box::new(Type::Enum("RuntimeError".to_string()))
            )
        );
        assert!(matches!(&module.functions[1].body[1], TypedStmt::Expr(_)));
        assert!(matches!(&module.functions[1].body[2], TypedStmt::Return(_)));
    }

    #[test]
    fn rejects_invalid_type_argument_arity() {
        let diagnostics =
            lower_source("fn main(values: list[int, string]) -> int:\n    return 1\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3067"]);
    }

    #[test]
    fn supports_unary_minus_division_and_modulo() {
        let module = lower_source("fn main() -> int:\n    base = -6 / 3\n    return base % 4\n")
            .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Int);
                assert!(matches!(
                    &value.kind,
                    TypedExprKind::Binary {
                        op: BinaryOp::Div,
                        ..
                    }
                ));
            }
            other => panic!("expected numeric bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Return(expr) => {
                assert_eq!(expr.ty, Type::Int);
                assert!(matches!(
                    &expr.kind,
                    TypedExprKind::Binary {
                        op: BinaryOp::Mod,
                        ..
                    }
                ));
            }
            other => panic!("expected modulo return, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_numeric_operands() {
        let diagnostics = lower_source("fn main() -> int:\n    return -\"oops\"\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3065"]);
    }

    #[test]
    fn supports_range_builtin() {
        let module = lower_source(
            "fn main() -> int:\n    values = range(1, 7, 2)\n    mut total = 0\n    for value in values:\n        total = total + value\n    return total\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "range")
                );
            }
            other => panic!("expected range bind, got {other:?}"),
        }
    }

    #[test]
    fn supports_for_in_over_core_iterables() {
        let module = lower_source(
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    for ch in \"go\":\n        total = total + len(ch)\n    mut store: dict = dict()\n    store = insert(store, \"alpha\", 1)\n    for key in store:\n        total = total + len(key)\n    return total\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::For {
                binding, iterable, ..
            } => {
                assert_eq!(binding, "value");
                assert_eq!(iterable.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected first loop, got {other:?}"),
        }

        match &module.functions[0].body[2] {
            TypedStmt::For {
                binding, iterable, ..
            } => {
                assert_eq!(binding, "ch");
                assert_eq!(iterable.ty, Type::String);
            }
            other => panic!("expected second loop, got {other:?}"),
        }

        match &module.functions[0].body[5] {
            TypedStmt::For {
                binding, iterable, ..
            } => {
                assert_eq!(binding, "key");
                assert_eq!(iterable.ty, Type::Dict(Box::new(Type::Int)));
            }
            other => panic!("expected dict loop, got {other:?}"),
        }
    }

    #[test]
    fn supports_break_and_continue_inside_loops() {
        let module = lower_source(
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3, 4]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::For { body, .. } => {
                assert!(matches!(
                    &body[0],
                    TypedStmt::If { then_body, .. } if matches!(&then_body[0], TypedStmt::Continue)
                ));
                assert!(matches!(
                    &body[2],
                    TypedStmt::If { then_body, .. } if matches!(&then_body[0], TypedStmt::Break)
                ));
            }
            other => panic!("expected for loop, got {other:?}"),
        }
    }

    #[test]
    fn supports_dict_assert_and_file_builtins() {
        let module = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    mut store: dict = dict()\n    store = insert(store, \"size\", 3)\n    assert(contains(store, \"size\"), \"missing size\")\n    write_file(\"out.txt\", read_file(\"in.txt\")?)?\n    return Result.Ok(store[\"size\"] + len(store))\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Dict(Box::new(Type::Unknown)));
            }
            other => panic!("expected dict bind, got {other:?}"),
        }

        match &module.functions[0].body[1] {
            TypedStmt::Assign { value, .. } => {
                assert_eq!(value.ty, Type::Dict(Box::new(Type::Int)));
            }
            other => panic!("expected dict assign, got {other:?}"),
        }

        assert_eq!(
            module.functions[0].return_type,
            Type::Result(
                Box::new(Type::Int),
                Box::new(Type::Enum("RuntimeError".to_string()))
            )
        );
    }

    #[test]
    fn supports_dict_view_builtins() {
        let module = lower_source(
            "fn main() -> int:\n    metrics: dict = {\"critical\": 5, \"ok\": 7, \"warn\": 2}\n    names = keys(metrics)\n    counts = values(metrics)\n    assert(names[0] == \"critical\", \"expected deterministic order\")\n    return len(names) + counts[0] + counts[1] + counts[2]\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::String)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "keys")
                );
            }
            other => panic!("expected keys bind, got {other:?}"),
        }

        match &module.functions[0].body[2] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
                assert!(
                    matches!(&value.kind, TypedExprKind::Call { callee, .. } if callee == "values")
                );
            }
            other => panic!("expected values bind, got {other:?}"),
        }

        assert_eq!(module.functions[0].return_type, Type::Int);
    }

    #[test]
    fn supports_channel_and_select_baseline() {
        let module = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel()\n    send(ch, 7)?\n    select:\n        received = recv(ch):\n            match received:\n                Result.Ok(value):\n                    return Result.Ok(value + 1)\n                Result.Err(error):\n                    return Result.Err(error)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[2] {
            TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 1);
                assert_eq!(arms[0].binding.as_deref(), Some("received"));
                assert!(matches!(arms[0].kind, TypedSelectArmKind::Recv { .. }));
            }
            other => panic!("expected select statement, got {other:?}"),
        }
    }

    #[test]
    fn supports_select_send_arms() {
        let module = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel(1)\n    select:\n        sent = send(ch, 7):\n            sent?\n            return Result.Ok(recv(ch)?)\n        default:\n            return Result.Ok(0)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[1] {
            TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 2);
                assert_eq!(arms[0].binding.as_deref(), Some("sent"));
                assert!(matches!(arms[0].kind, TypedSelectArmKind::Send { .. }));
                assert!(matches!(arms[1].kind, TypedSelectArmKind::Default));
            }
            other => panic!("expected select statement, got {other:?}"),
        }
    }

    #[test]
    fn supports_explicit_channel_capacity_baseline() {
        let module = lower_source("fn main() -> channel[int]:\n    return channel(0)\n")
            .expect("typing should succeed");

        assert_eq!(module.functions[0].return_type, Type::channel(Type::Int));

        match &module.functions[0].body[0] {
            TypedStmt::Return(expr) => {
                assert_eq!(expr.ty, Type::channel(Type::Unknown));
                assert!(matches!(
                    &expr.kind,
                    TypedExprKind::Call { callee, args } if callee == "channel" && args.len() == 1
                ));
            }
            other => panic!("expected channel return, got {other:?}"),
        }
    }

    #[test]
    fn supports_select_default_arm() {
        let module = lower_source(
            "fn main() -> int:\n    select:\n        default:\n            return 1\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 1);
                assert!(arms[0].binding.is_none());
                assert!(matches!(arms[0].kind, TypedSelectArmKind::Default));
            }
            other => panic!("expected select statement, got {other:?}"),
        }
    }

    #[test]
    fn supports_print_builtin_with_unit_return() {
        let module = lower_source("fn main() -> int:\n    print(\"gof\")\n    return 1\n")
            .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Expr(expr) => {
                assert_eq!(expr.ty, Type::Unit);
                assert!(
                    matches!(&expr.kind, TypedExprKind::Call { callee, .. } if callee == "print")
                );
            }
            other => panic!("expected print expression statement, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_printable_print_operands() {
        let diagnostics = lower_source(
            "fn noop() -> unit:\n    print(\"side\")\n    return print(\"ok\")\n\nfn main() -> int:\n    print(noop())\n    return 1\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3038"]);
    }

    #[test]
    fn rejects_invalid_append_operands() {
        let diagnostics = lower_source("fn main() -> list:\n    return append(1, 2)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3039"]);
    }

    #[test]
    fn rejects_invalid_contains_operands() {
        let diagnostics = lower_source("fn main() -> bool:\n    return contains(true, false)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3040"]);
    }

    #[test]
    fn rejects_invalid_split_operands() {
        let diagnostics = lower_source("fn main() -> list:\n    return split(1, \",\")\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3055"]);
    }

    #[test]
    fn rejects_invalid_join_operands() {
        let diagnostics = lower_source("fn main() -> string:\n    return join([1, 2], \",\")\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3056"]);
    }

    #[test]
    fn rejects_invalid_trim_operands() {
        let diagnostics = lower_source("fn main() -> string:\n    return trim(7)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3057"]);
    }

    #[test]
    fn rejects_invalid_starts_with_operands() {
        let diagnostics = lower_source("fn main() -> bool:\n    return starts_with(\"gof\", 1)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3058"]);
    }

    #[test]
    fn rejects_invalid_ends_with_operands() {
        let diagnostics =
            lower_source("fn main() -> bool:\n    return ends_with(false, \"gof\")\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3059"]);
    }

    #[test]
    fn rejects_invalid_parse_int_operands() {
        let diagnostics =
            lower_source("fn main() -> Result[int, RuntimeError]:\n    return parse_int(1)\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3060"]);
    }

    #[test]
    fn rejects_invalid_sleep_operands() {
        let diagnostics = lower_source("fn main() -> unit:\n    sleep(\"soon\")\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3085"]);
    }

    #[test]
    fn rejects_invalid_timeout_token_operands() {
        let diagnostics =
            lower_source("fn main() -> cancel_token:\n    return timeout_token(\"soon\")\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3093"]);
    }

    #[test]
    fn rejects_negative_timeout_cancellation_durations() {
        let diagnostics =
            lower_source("fn main() -> unit:\n    cancel_after(cancel_token(), -1)\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3094"]);
    }

    #[test]
    fn rejects_invalid_channel_capacity_operands() {
        let diagnostics =
            lower_source("fn main() -> channel[int]:\n    return channel(\"wide\")\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3096"]);
    }

    #[test]
    fn rejects_negative_channel_capacity_values() {
        let diagnostics = lower_source("fn main() -> channel[int]:\n    return channel(-1)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3097"]);
    }

    #[test]
    fn rejects_invalid_to_string_operands() {
        let diagnostics = lower_source("fn main() -> string:\n    return to_string(channel())\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3062"]);
    }

    #[test]
    fn rejects_invalid_range_operands() {
        let diagnostics = lower_source("fn main() -> list:\n    return range(\"bad\")\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3063"]);
    }

    #[test]
    fn rejects_invalid_first_operands() {
        let diagnostics =
            lower_source("fn main() -> Result[int, RuntimeError]:\n    return first(1)\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_slice_operands() {
        let diagnostics = lower_source(
            "fn main() -> Result[list[int], RuntimeError]:\n    return slice([1, 2, 3], \"start\", 2)\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_sort_operands() {
        let diagnostics =
            lower_source("fn main() -> list[bool]:\n    return sort([true, false])\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_min_operands() {
        let diagnostics = lower_source(
            "fn main() -> Result[int, RuntimeError]:\n    return min([true, false])\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_invalid_max_operands() {
        let diagnostics =
            lower_source("fn main() -> Result[int, RuntimeError]:\n    return max(1)\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3088"]);
    }

    #[test]
    fn rejects_zero_range_step() {
        let diagnostics = lower_source("fn main() -> list:\n    return range(0, 5, 0)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3064"]);
    }

    #[test]
    fn rejects_invalid_keys_operands() {
        let diagnostics = lower_source("fn main() -> list:\n    return keys(1)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3051"]);
    }

    #[test]
    fn rejects_invalid_values_operands() {
        let diagnostics = lower_source("fn main() -> list:\n    return values(true)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3052"]);
    }

    #[test]
    fn rejects_invalid_assert_operands() {
        let diagnostics = lower_source("fn main() -> unit:\n    return assert(1)\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3041"]);
    }

    #[test]
    fn rejects_invalid_select_operations() {
        let diagnostics = lower_source(
            "fn main() -> int:\n    select:\n        print(\"bad\"):\n            return 1\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3047"]);
    }

    #[test]
    fn rejects_duplicate_select_default_arms() {
        let diagnostics = lower_source(
            "fn main() -> int:\n    select:\n        default:\n            return 1\n        default:\n            return 2\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3095"]);
    }

    #[test]
    fn rejects_invalid_for_iterables() {
        let diagnostics =
            lower_source("fn main() -> int:\n    for value in 42:\n        return value\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3048"]);
    }

    #[test]
    fn rejects_break_outside_loops() {
        let diagnostics = lower_source("fn main() -> int:\n    break\n    return 0\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3053"]);
    }

    #[test]
    fn rejects_continue_outside_loops() {
        let diagnostics = lower_source("fn main() -> int:\n    continue\n    return 0\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3054"]);
    }

    #[test]
    fn supports_struct_contracts_and_field_access() {
        let module = lower_source(
            "struct Point:\n    x: int\n    y: int\n\nfn magnitude(point: Point) -> int:\n    return point.x + point.y\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return magnitude(point)\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.structs.len(), 1);
        assert_eq!(module.structs[0].name, "Point");
        assert_eq!(module.structs[0].fields[0].ty, Type::Int);
        assert_eq!(
            module.functions[0].params[0].ty,
            Type::Struct("Point".to_string())
        );
        assert_eq!(module.functions[1].return_type, Type::Int);
    }

    #[test]
    fn supports_enum_contracts_and_variant_references() {
        let module = lower_source(
            "enum Status:\n    Ready\n    Busy\n\nfn is_ready(status: Status) -> bool:\n    return status == Status.Ready\n\nfn main() -> bool:\n    current: Status = Status.Busy\n    return is_ready(current)\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.enums.len(), 1);
        assert_eq!(module.enums[0].name, "Status");
        assert_eq!(
            module.functions[0].params[0].ty,
            Type::Enum("Status".to_string())
        );
        assert_eq!(module.functions[0].return_type, Type::Bool);
        assert_eq!(module.functions[1].return_type, Type::Bool);

        match &module.functions[0].body[0] {
            TypedStmt::Return(expr) => assert_eq!(expr.ty, Type::Bool),
            other => panic!("expected enum comparison return, got {other:?}"),
        }
    }

    #[test]
    fn supports_explicit_comparison_semantics() {
        let module = lower_source(
            "struct Snapshot:\n    count: int\n    label: string\n\nenum Stage:\n    Draft\n    Published(version: int)\n\nfn main() -> Result[bool, RuntimeError]:\n    left = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    right = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    snapshot_a: Snapshot = Snapshot(2, \"beta\")\n    snapshot_b: Snapshot = Snapshot(2, \"beta\")\n    stage_a: Stage = Stage.Published(3)\n    stage_b: Stage = Stage.Published(3)\n    ok_a: Result[int, RuntimeError] = Result.Ok(7)\n    ok_b: Result[int, RuntimeError] = Result.Ok(7)\n    return Result.Ok(\"alpha\" < \"beta\" and left == right and snapshot_a == snapshot_b and stage_a == stage_b and ok_a == ok_b and sleep(0) == sleep(0) and [1, 2] == [1, 2] and {\"ok\": 2} == {\"ok\": 2})\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.structs[0].name, "Snapshot");
        assert_eq!(module.enums[0].name, "Stage");
        assert_eq!(
            module.functions[0].return_type,
            Type::result(Type::Bool, Type::Enum("RuntimeError".to_string()))
        );

        match &module.functions[0].body[0] {
            TypedStmt::Bind { value, .. } => assert_eq!(value.ty, Type::Json),
            other => panic!("expected propagated json bind, got {other:?}"),
        }
        match &module.functions[0].body[8] {
            TypedStmt::Return(expr) => assert!(matches!(expr.ty, Type::Result(_, _))),
            other => panic!("expected result-returning comparison expression, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_struct_field_access() {
        let diagnostics = lower_source(
            "struct Point:\n    x: int\n\nfn main() -> int:\n    point = Point(3)\n    return point.y\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3022"]);
    }

    #[test]
    fn rejects_field_access_on_non_struct() {
        let diagnostics = lower_source("fn main() -> int:\n    return 42.value\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3024"]);
    }

    #[test]
    fn rejects_duplicate_struct_fields() {
        let diagnostics = lower_source(
            "struct Point:\n    x: int\n    x: int\n\nfn main() -> int:\n    point = Point(1, 2)\n    return point.x\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3023"]);
    }

    #[test]
    fn rejects_duplicate_enum_variants() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n    Ready\n\nfn main() -> bool:\n    return Status.Ready == Status.Ready\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3027"]);
    }

    #[test]
    fn rejects_unknown_enum_variant() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n\nfn main() -> bool:\n    return Status.Busy == Status.Ready\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3028"]);
    }

    #[test]
    fn rejects_incompatible_equality_operands() {
        let diagnostics = lower_source("fn main() -> bool:\n    return 1 == \"1\"\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3087"]);
    }

    #[test]
    fn rejects_non_orderable_values() {
        let diagnostics = lower_source("fn main() -> bool:\n    return [1, 2] < [1, 2]\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3087"]);
    }

    #[test]
    fn rejects_struct_equality_when_fields_are_not_comparable() {
        let diagnostics = lower_source(
            "struct Worker:\n    inbox: channel[int]\n\nfn main() -> bool:\n    left: Worker = Worker(channel())\n    right: Worker = Worker(channel())\n    return left == right\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3087"]);
    }

    #[test]
    fn rejects_result_equality_when_payload_is_not_comparable() {
        let diagnostics = lower_source(
            "fn main() -> bool:\n    left: Result[channel[int], RuntimeError] = Result.Ok(channel())\n    right: Result[channel[int], RuntimeError] = Result.Ok(channel())\n    return left == right\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3087"]);
    }

    #[test]
    fn rejects_enum_calls() {
        let diagnostics =
            lower_source("enum Status:\n    Ready\n\nfn main() -> Status:\n    return Status()\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3004"]);
    }

    #[test]
    fn supports_exhaustive_match_over_enum_variants() {
        let module = lower_source(
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 1\n        Status.Busy:\n            return 2\n\nfn main() -> int:\n    return score(Status.Busy)\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[0] {
            TypedStmt::Match { value, arms } => {
                assert_eq!(value.ty, Type::Enum("Status".to_string()));
                assert_eq!(arms.len(), 2);
            }
            other => panic!("expected match statement, got {other:?}"),
        }
    }

    #[test]
    fn supports_payload_enums_and_destructuring_matches() {
        let module = lower_source(
            "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn score(state: JobState) -> int:\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n\nfn main() -> int:\n    current: JobState = JobState.Running(42)\n    return score(current)\n",
        )
        .expect("typing should succeed");

        assert_eq!(module.enums[0].variants[1].name, "Running");
        assert_eq!(module.enums[0].variants[1].fields.len(), 1);
        assert_eq!(module.enums[0].variants[1].fields[0].ty, Type::Int);

        match &module.functions[0].body[0] {
            TypedStmt::Match { arms, .. } => {
                assert!(matches!(
                    &arms[1].pattern,
                    TypedMatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "JobState"
                        && variant == "Running"
                        && bindings.len() == 1
                        && bindings[0].name == "pid"
                        && bindings[0].ty == Type::Int
                ));
            }
            other => panic!("expected payload match, got {other:?}"),
        }

        match &module.functions[1].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Enum("JobState".to_string()));
                assert!(matches!(
                    &value.kind,
                    TypedExprKind::EnumVariant { enum_name, variant, args }
                        if enum_name == "JobState" && variant == "Running" && args.len() == 1
                ));
            }
            other => panic!("expected enum constructor bind, got {other:?}"),
        }
    }

    #[test]
    fn rejects_match_on_non_enum_values() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n\nfn main() -> int:\n    value = 1\n    match value:\n        Status.Ready:\n            return 1\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3032"]);
    }

    #[test]
    fn rejects_non_exhaustive_match_over_enum() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> int:\n    current: Status = Status.Ready\n    match current:\n        Status.Ready:\n            return 1\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3033"]);
    }

    #[test]
    fn rejects_duplicate_match_arms() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> int:\n    current: Status = Status.Ready\n    match current:\n        Status.Ready:\n            return 1\n        Status.Ready:\n            return 2\n        Status.Busy:\n            return 3\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3030"]);
    }

    #[test]
    fn rejects_match_arms_from_different_enums() {
        let diagnostics = lower_source(
            "enum Status:\n    Ready\n\nenum Mode:\n    Fast\n\nfn main() -> int:\n    current: Status = Status.Ready\n    match current:\n        Mode.Fast:\n            return 1\n        Status.Ready:\n            return 2\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3031"]);
    }

    #[test]
    fn rejects_payload_constructor_arity_mismatches() {
        let diagnostics = lower_source(
            "enum JobState:\n    Running(pid: int)\n\nfn main() -> JobState:\n    return JobState.Running()\n",
        )
        .expect_err("payload constructor arity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3069"]);
    }

    #[test]
    fn rejects_payload_match_destructuring_arity_mismatches() {
        let diagnostics = lower_source(
            "enum JobState:\n    Running(pid: int)\n\nfn main() -> int:\n    state: JobState = JobState.Running(7)\n    match state:\n        JobState.Running(pid, extra):\n            return pid\n",
        )
        .expect_err("payload match arity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3071"]);
    }

    #[test]
    fn supports_result_constructors_propagation_and_match() {
        let module = lower_source(
            "enum MathError:\n    TooSmall\n    NotEven(value: int)\n\nfn halve(value: int) -> Result[int, MathError]:\n    if value < 2:\n        return Result.Err(MathError.TooSmall)\n    if value % 2 != 0:\n        return Result.Err(MathError.NotEven(value))\n    return Result.Ok(value / 2)\n\nfn compute() -> Result[int, MathError]:\n    half = halve(84)?\n    return Result.Ok(half)\n\nfn main() -> int:\n    outcome: Result[int, MathError] = compute()\n    match outcome:\n        Result.Ok(value):\n            return value\n        Result.Err(error):\n            match error:\n                MathError.TooSmall:\n                    return 0\n                MathError.NotEven(value):\n                    return value\n",
        )
        .expect("typing should succeed");

        let result_type = Type::result(Type::Int, Type::Enum("MathError".to_string()));
        assert_eq!(module.functions[0].return_type, result_type);
        assert_eq!(module.functions[1].return_type, result_type);

        match &module.functions[1].body[0] {
            TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Int);
                assert!(matches!(value.kind, TypedExprKind::Propagate { .. }));
            }
            other => panic!("expected propagation bind, got {other:?}"),
        }

        match &module.functions[2].body[1] {
            TypedStmt::Match { value, arms } => {
                assert_eq!(value.ty, result_type);
                assert!(matches!(
                    &arms[0].pattern,
                    TypedMatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "Result"
                        && variant == "Ok"
                        && bindings.len() == 1
                        && bindings[0].name == "value"
                        && bindings[0].ty == Type::Int
                ));
                assert!(matches!(
                    &arms[1].pattern,
                    TypedMatchPattern::EnumVariant {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } if enum_name == "Result"
                        && variant == "Err"
                        && bindings.len() == 1
                        && bindings[0].name == "error"
                        && bindings[0].ty == Type::Enum("MathError".to_string())
                ));
            }
            other => panic!("expected result match, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_result_type_arity() {
        let diagnostics = lower_source(
            "fn main() -> int:\n    value: Result[int] = Result.Ok(1)\n    return 0\n",
        )
        .expect_err("result type arity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3067"]);
    }

    #[test]
    fn rejects_unknown_result_variants() {
        let diagnostics =
            lower_source("fn main() -> Result[int, string]:\n    return Result.Maybe(1)\n")
                .expect_err("unknown result variant should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3072"]);
    }

    #[test]
    fn rejects_invalid_result_payload_arity() {
        let diagnostics =
            lower_source("fn main() -> Result[int, string]:\n    return Result.Ok()\n")
                .expect_err("result payload arity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3073"]);
    }

    #[test]
    fn rejects_invalid_propagate_operands() {
        let diagnostics = lower_source(
            "fn main() -> Result[int, string]:\n    value = 1?\n    return Result.Ok(value)\n",
        )
        .expect_err("propagate operand should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3074"]);
    }

    #[test]
    fn rejects_propagation_without_result_return_contract() {
        let diagnostics = lower_source(
            "fn parse_port() -> Result[int, string]:\n    return Result.Ok(41)\n\nfn main() -> int:\n    port = parse_port()?\n    return port\n",
        )
        .expect_err("non-result return contract should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3075"]);
    }

    #[test]
    fn rejects_result_match_payload_arity_mismatches() {
        let diagnostics = lower_source(
            "fn main() -> int:\n    outcome: Result[int, string] = Result.Ok(7)\n    match outcome:\n        Result.Ok(value, extra):\n            return value\n        Result.Err(error):\n            return len(error)\n",
        )
        .expect_err("result match arity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3071"]);
    }

    #[test]
    fn supports_receiver_methods_on_structs() {
        let module = lower_source(
            "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n",
        )
        .expect("typing should succeed");

        assert_eq!(
            module.functions[0].receiver_type,
            Some(Type::Struct("Point".to_string()))
        );
        assert_eq!(module.functions[0].symbol_name, "Point.total");

        match &module.functions[1].body[1] {
            TypedStmt::Return(expr) => match &expr.kind {
                TypedExprKind::MethodCall { symbol_name, .. } => {
                    assert_eq!(symbol_name, "Point.total");
                    assert_eq!(expr.ty, Type::Int);
                }
                other => panic!("expected method call, got {other:?}"),
            },
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_method_receiver_contract() {
        let diagnostics = lower_source(
            "struct Point:\n    x: int\n\nfn Point.total(self: int) -> int:\n    return self\n\nfn main() -> int:\n    point: Point = Point(3)\n    return point.total()\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3035"]);
    }

    #[test]
    fn rejects_unknown_methods_on_structs() {
        let diagnostics = lower_source(
            "struct Point:\n    x: int\n\nfn main() -> int:\n    point: Point = Point(3)\n    return point.total()\n",
        )
        .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3036"]);
    }

    #[test]
    fn rejects_method_calls_on_non_struct_values() {
        let diagnostics = lower_source("fn main() -> int:\n    return 42.total()\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3037"]);
    }

    #[test]
    fn supports_logical_operators() {
        let module = lower_source("fn main() -> bool:\n    return not false and true or false\n")
            .expect("typing should succeed");

        assert_eq!(module.functions[0].return_type, Type::Bool);
    }

    #[test]
    fn rejects_non_bool_logical_operands() {
        let diagnostics = lower_source("fn main() -> bool:\n    return 1 and true\n")
            .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3025"]);
    }

    #[test]
    fn rejects_non_bool_not_operand() {
        let diagnostics =
            lower_source("fn main() -> bool:\n    return not 1\n").expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3026"]);
    }
}
