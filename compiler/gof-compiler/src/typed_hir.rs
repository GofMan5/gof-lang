use crate::ast::{BinaryOp, UnaryOp};
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::hir::{
    HirEnum, HirExpr, HirFunction, HirMatchArm, HirModule, HirSelectArm, HirStmt, HirStruct,
    HirTypeRef,
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
    Struct(String),
    Enum(String),
    List(Box<Type>),
    Dict(Box<Type>),
    Channel(Box<Type>),
    Task(Box<Type>),
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

    fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    fn display_name(&self) -> String {
        match self {
            Self::Int => "int".to_string(),
            Self::String => "string".to_string(),
            Self::Bool => "bool".to_string(),
            Self::Struct(name) => name.clone(),
            Self::Enum(name) => name.clone(),
            Self::List(inner) => format!("list[{}]", inner.display_name()),
            Self::Dict(inner) => format!("dict[{}]", inner.display_name()),
            Self::Channel(inner) => format!("channel[{}]", inner.display_name()),
            Self::Task(inner) => format!("task[{}]", inner.display_name()),
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
    pub pattern: TypedExpr,
    pub body: Vec<TypedStmt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedSelectArm {
    pub binding: Option<String>,
    pub operation: TypedExpr,
    pub body: Vec<TypedStmt>,
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
}

#[derive(Debug, Clone)]
struct LocalBinding {
    mutable: bool,
    ty: Type,
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
    BuiltinAssert,
    BuiltinReadFile,
    BuiltinWriteFile,
    BuiltinDict,
    BuiltinInsert,
    BuiltinChannel,
    BuiltinSend,
    BuiltinRecv,
    Function,
    Struct,
    Enum,
    Unknown,
}

pub fn lower(module: &HirModule) -> Result<TypedModule, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let known_structs = module
        .structs
        .iter()
        .map(|decl| decl.name.clone())
        .collect::<HashSet<_>>();
    let known_enums = module
        .enums
        .iter()
        .map(|decl| decl.name.clone())
        .collect::<HashSet<_>>();
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
    let enum_signatures = module
        .enums
        .iter()
        .map(|decl| {
            (
                decl.name.clone(),
                lower_enum_signature(decl, &mut diagnostics),
            )
        })
        .collect::<HashMap<_, _>>();
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
        false,
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

fn lower_enum_signature(decl: &HirEnum, diagnostics: &mut Diagnostics) -> EnumSignature {
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
    nested_scope: bool,
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
                true,
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
                true,
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
                true,
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
                false,
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
    source_path: &Path,
    value: &TypedExpr,
    span: Span,
) -> Vec<TypedMatchArm> {
    let match_enum_name = match &value.ty {
        Type::Enum(name) => Some(name.clone()),
        Type::Unknown => None,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3032",
                    "`match` currently requires an enum value",
                    format!("this match target resolves to `{}`", other.display_name()),
                    value.span,
                )
                .with_fix_it("match over a value whose type is a known enum")
                .with_source_path(source_path.to_path_buf()),
            );
            None
        }
    };

    let mut seen_variants = HashSet::new();
    let typed_arms = arms
        .iter()
        .map(|arm| {
            let pattern = lower_expr(
                &arm.pattern,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                source_path,
            );
            validate_match_pattern(
                &pattern,
                match_enum_name.as_deref(),
                &mut seen_variants,
                diagnostics,
                source_path,
            );
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
                true,
                source_path,
            );
            TypedMatchArm { pattern, body }
        })
        .collect::<Vec<_>>();

    if let Some(enum_name) = match_enum_name {
        ensure_match_exhaustive(
            &enum_name,
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
    source_path: &Path,
    span: Span,
) -> Vec<TypedSelectArm> {
    if arms.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3047",
                "`select` requires at least one arm",
                "the bootstrap select model needs one or more `recv(channel)` arms",
                span,
            )
            .with_fix_it("add at least one select arm like `value = recv(ch):`")
            .with_source_path(source_path.to_path_buf()),
        );
    }

    arms.iter()
        .map(|arm| {
            let operation = lower_expr(
                &arm.operation,
                scopes,
                signatures,
                method_signatures,
                known_structs,
                known_enums,
                struct_signatures,
                enum_signatures,
                diagnostics,
                source_path,
            );
            validate_select_operation(&operation, diagnostics, source_path);

            let body = {
                scopes.push();
                if let Some(binding) = &arm.binding {
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
                                ty: operation.ty.clone(),
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
                    false,
                    source_path,
                );
                scopes.pop();
                body
            };

            TypedSelectArm {
                binding: arm.binding.clone(),
                operation,
                body,
            }
        })
        .collect()
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

fn validate_match_pattern(
    pattern: &TypedExpr,
    match_enum_name: Option<&str>,
    seen_variants: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    let Some(expected_enum) = match_enum_name else {
        return;
    };

    let TypedExprKind::EnumVariant { enum_name, variant } = &pattern.kind else {
        diagnostics.push(
            Diagnostic::error(
                "GOF3031",
                "match arms must use enum variants",
                "each match arm pattern must be written as `EnumName.Variant`",
                pattern.span,
            )
            .with_fix_it("replace this pattern with a unit enum variant like `Status.Ready`")
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    };

    if enum_name != expected_enum {
        diagnostics.push(
            Diagnostic::error(
                "GOF3031",
                "match arm uses a variant from a different enum",
                format!(
                    "this match targets `{expected_enum}`, but the arm pattern belongs to `{enum_name}`"
                ),
                pattern.span,
            )
            .with_fix_it(format!("use a `{expected_enum}.Variant` pattern here"))
            .with_source_path(source_path.to_path_buf()),
        );
        return;
    }

    if !seen_variants.insert(variant.clone()) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3030",
                format!("duplicate match arm for `{expected_enum}.{variant}`"),
                "each unit enum variant can appear only once in a match over the same enum",
                pattern.span,
            )
            .with_fix_it("remove the duplicate arm or replace it with another enum variant")
            .with_source_path(source_path.to_path_buf()),
        );
    }
}

fn ensure_match_exhaustive(
    enum_name: &str,
    seen_variants: &HashSet<String>,
    enum_signatures: &HashMap<String, EnumSignature>,
    diagnostics: &mut Diagnostics,
    span: Span,
    source_path: &Path,
) {
    let Some(signature) = enum_signatures.get(enum_name) else {
        return;
    };

    let missing = signature
        .variants
        .iter()
        .filter(|variant| !seen_variants.contains(&variant.name))
        .map(|variant| variant.name.clone())
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3033",
                format!("non-exhaustive match over `{enum_name}`"),
                format!(
                    "missing arm(s): {}",
                    missing
                        .iter()
                        .map(|variant| format!("{enum_name}.{variant}"))
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
                    | CallKind::BuiltinAssert
                    | CallKind::BuiltinReadFile
                    | CallKind::BuiltinWriteFile
                    | CallKind::BuiltinDict
                    | CallKind::BuiltinInsert
                    | CallKind::BuiltinChannel
                    | CallKind::BuiltinSend
                    | CallKind::BuiltinRecv
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
                if scopes.get(name).is_none() && enum_signatures.contains_key(name) {
                    let ty = infer_enum_variant_type(
                        name,
                        field,
                        diagnostics,
                        *span,
                        enum_signatures,
                        source_path,
                    );
                    return TypedExpr {
                        kind: TypedExprKind::EnumVariant {
                            enum_name: name.clone(),
                            variant: field.clone(),
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
                source_path,
            );
            validate_binary_expr(*op, &lhs, &rhs, diagnostics, source_path);
            let ty = infer_binary_type(&lhs.ty, *op, &rhs.ty);
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
        CallKind::BuiltinAssert => {
            validate_assert_call(args, span, diagnostics, source_path);
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
        CallKind::BuiltinChannel => {
            validate_channel_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinSend => {
            validate_send_call(args, span, diagnostics, source_path);
        }
        CallKind::BuiltinRecv => {
            validate_recv_call(args, span, diagnostics, source_path);
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
                "calls currently resolve only to top-level functions, builtin helpers like `len`, `print`, `append`, `contains`, `assert`, `read_file`, `write_file`, `dict`, `insert`, `channel`, `send`, `recv`, or struct constructors; enums use `EnumName.Variant`",
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
        "int" => Type::Int,
        "string" => Type::String,
        "bool" => Type::Bool,
        "list" => Type::list(Type::Unknown),
        "dict" => Type::dict(Type::Unknown),
        "channel" => Type::channel(Type::Unknown),
        "task" => Type::task(Type::Unknown),
        "unit" => Type::Unit,
        name if known_structs.contains(name) => Type::Struct(name.to_string()),
        name if known_enums.contains(name) => Type::Enum(name.to_string()),
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

fn resolve_call_kind(
    callee: &str,
    signatures: &HashMap<String, FunctionSignature>,
    struct_signatures: &HashMap<String, StructSignature>,
    enum_signatures: &HashMap<String, EnumSignature>,
) -> CallKind {
    if callee == "len" {
        CallKind::BuiltinLen
    } else if callee == "print" {
        CallKind::BuiltinPrint
    } else if callee == "append" {
        CallKind::BuiltinAppend
    } else if callee == "contains" {
        CallKind::BuiltinContains
    } else if callee == "assert" {
        CallKind::BuiltinAssert
    } else if callee == "read_file" {
        CallKind::BuiltinReadFile
    } else if callee == "write_file" {
        CallKind::BuiltinWriteFile
    } else if callee == "dict" {
        CallKind::BuiltinDict
    } else if callee == "insert" {
        CallKind::BuiltinInsert
    } else if callee == "channel" {
        CallKind::BuiltinChannel
    } else if callee == "send" {
        CallKind::BuiltinSend
    } else if callee == "recv" {
        CallKind::BuiltinRecv
    } else if signatures.contains_key(callee) {
        CallKind::Function
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
        CallKind::BuiltinAssert => Type::Unit,
        CallKind::BuiltinReadFile => Type::String,
        CallKind::BuiltinWriteFile => Type::Unit,
        CallKind::BuiltinDict => Type::dict(Type::Unknown),
        CallKind::BuiltinInsert => infer_insert_return_type(args),
        CallKind::BuiltinChannel => Type::channel(Type::Unknown),
        CallKind::BuiltinSend => Type::Unit,
        CallKind::BuiltinRecv => infer_recv_return_type(args),
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
    if matches!(op, UnaryOp::Not) && !matches!(value.ty, Type::Bool | Type::Unknown) {
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
}

fn infer_unary_type(op: UnaryOp, value: &Type) -> Type {
    match (op, value) {
        (UnaryOp::Not, Type::Bool) => Type::Bool,
        (UnaryOp::Not, Type::Unknown) => Type::Unknown,
        _ => Type::Unknown,
    }
}

fn validate_binary_expr(
    op: BinaryOp,
    lhs: &TypedExpr,
    rhs: &TypedExpr,
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
    }
}

fn infer_binary_type(lhs: &Type, op: BinaryOp, rhs: &Type) -> Type {
    match (lhs, op, rhs) {
        (Type::Int, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul, Type::Int) => Type::Int,
        (Type::String, BinaryOp::Add, Type::String) => Type::String,
        (Type::Bool, BinaryOp::And | BinaryOp::Or, Type::Bool) => Type::Bool,
        (
            Type::Int,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge,
            Type::Int,
        ) => Type::Bool,
        (Type::String, BinaryOp::Eq | BinaryOp::Ne, Type::String) => Type::Bool,
        (Type::Bool, BinaryOp::Eq | BinaryOp::Ne, Type::Bool) => Type::Bool,
        (Type::List(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::List(rhs))
            if types_compatible(lhs, rhs) =>
        {
            Type::Bool
        }
        (Type::Dict(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::Dict(rhs))
            if types_compatible(lhs, rhs) =>
        {
            Type::Bool
        }
        (Type::Struct(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::Struct(rhs)) if lhs == rhs => {
            Type::Bool
        }
        (Type::Enum(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::Enum(rhs)) if lhs == rhs => Type::Bool,
        _ => Type::Unknown,
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

fn infer_enum_variant_type(
    enum_name: &str,
    variant: &str,
    diagnostics: &mut Diagnostics,
    span: Span,
    enum_signatures: &HashMap<String, EnumSignature>,
    source_path: &Path,
) -> Type {
    let Some(signature) = enum_signatures.get(enum_name) else {
        return Type::Unknown;
    };

    if signature
        .variants
        .iter()
        .any(|candidate| candidate.name == variant)
    {
        Type::Enum(enum_name.to_string())
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
        Type::Unknown
    }
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
                merge_return_candidate(
                    function_name,
                    returns,
                    &expr.ty,
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
            TypedStmt::Bind { .. } | TypedStmt::Assign { .. } | TypedStmt::Expr(_) => {}
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

fn validate_channel_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if !args.is_empty() {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `channel`",
                format!("expected 0 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `channel()` without arguments")
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
    if args.len() != 2 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `send`",
                format!("expected 2 arguments, got {}", args.len()),
                span,
            )
            .with_fix_it("call `send(channel_value, item)`")
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
}

fn validate_recv_call(
    args: &[TypedExpr],
    span: Span,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    if args.len() != 1 {
        diagnostics.push(
            Diagnostic::error(
                "GOF3005",
                "wrong number of arguments for `recv`",
                format!("expected 1 argument, got {}", args.len()),
                span,
            )
            .with_fix_it("call `recv(channel_value)`")
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

fn infer_recv_return_type(args: &[TypedExpr]) -> Type {
    if args.len() != 1 {
        return Type::Unknown;
    }

    match &args[0].ty {
        Type::Channel(inner) => inner.as_ref().clone(),
        _ => Type::Unknown,
    }
}

fn validate_select_operation(
    operation: &TypedExpr,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) {
    match &operation.kind {
        TypedExprKind::Call { callee, args } if callee == "recv" && args.len() == 1 => {}
        TypedExprKind::Call { callee, .. } => diagnostics.push(
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(channel)` operations",
                format!("this arm uses `{callee}(...)` instead"),
                operation.span,
            )
            .with_fix_it("replace the arm operation with `recv(channel_value)`")
            .with_source_path(source_path.to_path_buf()),
        ),
        _ => diagnostics.push(
            Diagnostic::error(
                "GOF3047",
                "`select` arms currently require `recv(channel)` operations",
                "select arms must be written as `recv(channel):` or `value = recv(channel):`",
                operation.span,
            )
            .with_fix_it("replace this arm with a `recv(channel)` operation")
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn is_printable_type(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::String | Type::Bool | Type::Struct(_) | Type::Enum(_) | Type::Unknown => {
            true
        }
        Type::List(inner) | Type::Dict(inner) => !matches!(
            inner.as_ref(),
            Type::Task(_) | Type::Unit | Type::Channel(_)
        ),
        Type::Channel(_) | Type::Task(_) | Type::Unit => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{Type, TypedExprKind, TypedStmt, lower};
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
    fn supports_dict_assert_and_file_builtins() {
        let module = lower_source(
            "fn main() -> int:\n    mut store: dict = dict()\n    store = insert(store, \"size\", 3)\n    assert(contains(store, \"size\"), \"missing size\")\n    write_file(\"out.txt\", read_file(\"in.txt\"))\n    return store[\"size\"] + len(store)\n",
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

        assert_eq!(module.functions[0].return_type, Type::Int);
    }

    #[test]
    fn supports_channel_and_select_baseline() {
        let module = lower_source(
            "fn main() -> int:\n    ch: channel = channel()\n    send(ch, 7)\n    select:\n        value = recv(ch):\n            return value + 1\n",
        )
        .expect("typing should succeed");

        match &module.functions[0].body[2] {
            TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 1);
                assert_eq!(arms[0].binding.as_deref(), Some("value"));
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
    fn rejects_invalid_for_iterables() {
        let diagnostics =
            lower_source("fn main() -> int:\n    for value in 42:\n        return value\n")
                .expect_err("typing should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3048"]);
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
    fn rejects_match_on_non_enum_values() {
        let diagnostics = lower_source(
            "fn main() -> int:\n    value = 1\n    match value:\n        value:\n            return 1\n",
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
