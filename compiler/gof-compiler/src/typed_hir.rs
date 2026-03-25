use crate::ast::BinaryOp;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::hir::{HirExpr, HirFunction, HirModule, HirStmt, HirStruct, HirTypeRef};
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
    List(Box<Type>),
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

    fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    fn display_name(&self) -> String {
        match self {
            Self::Int => "int".to_string(),
            Self::String => "string".to_string(),
            Self::Bool => "bool".to_string(),
            Self::Struct(name) => name.clone(),
            Self::List(inner) => format!("list[{}]", inner.display_name()),
            Self::Task(inner) => format!("task[{}]", inner.display_name()),
            Self::Unknown => "unknown".to_string(),
            Self::Unit => "unit".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedModule {
    pub structs: Vec<TypedStruct>,
    pub functions: Vec<TypedFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypedFunction {
    pub id: usize,
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
pub struct TypedStructField {
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
    List {
        items: Vec<TypedExpr>,
    },
    Call {
        callee: String,
        args: Vec<TypedExpr>,
    },
    StructInit {
        name: String,
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
    Binary {
        lhs: Box<TypedExpr>,
        op: BinaryOp,
        rhs: Box<TypedExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionSignature {
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
struct StructFieldSignature {
    name: String,
    ty: Type,
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
    Function,
    Struct,
    Unknown,
}

pub fn lower(module: &HirModule) -> Result<TypedModule, Diagnostics> {
    let mut diagnostics = Diagnostics::default();
    let known_structs = module
        .structs
        .iter()
        .map(|decl| decl.name.clone())
        .collect::<HashSet<_>>();
    let struct_signatures = module
        .structs
        .iter()
        .map(|decl| {
            (
                decl.name.clone(),
                lower_struct_signature(decl, &known_structs, &mut diagnostics),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut signatures = module
        .functions
        .iter()
        .map(|function| {
            let declared_return_type = resolve_type_annotation(
                function.return_type.as_ref(),
                &known_structs,
                &function.source_path,
                &mut diagnostics,
            );
            let declared_return_type = if function.return_type.is_some() {
                Some(declared_return_type)
            } else {
                None
            };
            (
                function.name.clone(),
                FunctionSignature {
                    arity: function.params.len(),
                    param_types: function
                        .params
                        .iter()
                        .map(|param| {
                            resolve_type_annotation(
                                param.ty.as_ref(),
                                &known_structs,
                                &function.source_path,
                                &mut diagnostics,
                            )
                        })
                        .collect(),
                    declared_return_type: declared_return_type.clone(),
                    return_type: declared_return_type.unwrap_or(Type::Unknown),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    for _ in 0..=module.functions.len() {
        let typed_functions = module
            .functions
            .iter()
            .map(|function| {
                lower_function(
                    function,
                    &signatures,
                    &known_structs,
                    &struct_signatures,
                    &mut Diagnostics::default(),
                )
            })
            .collect::<Vec<_>>();

        let mut changed = false;
        for function in &typed_functions {
            let signature = signatures
                .get_mut(&function.name)
                .expect("all lowered functions must have a matching signature entry");
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
    let functions = module
        .functions
        .iter()
        .map(|function| {
            lower_function(
                function,
                &signatures,
                &known_structs,
                &struct_signatures,
                &mut diagnostics,
            )
        })
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        Ok(TypedModule { structs, functions })
    } else {
        Err(diagnostics)
    }
}

fn lower_function(
    function: &HirFunction,
    signatures: &HashMap<String, FunctionSignature>,
    known_structs: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    diagnostics: &mut Diagnostics,
) -> TypedFunction {
    let signature = signatures
        .get(&function.name)
        .expect("each function should have a signature during lowering");
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
        known_structs,
        struct_signatures,
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
        name: function.name.clone(),
        params,
        return_type,
        body,
    }
}

fn lower_struct_signature(
    decl: &HirStruct,
    known_structs: &HashSet<String>,
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
                        &decl.source_path,
                        diagnostics,
                    ),
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

fn lower_block(
    stmts: &[HirStmt],
    scopes: &mut ScopeStack,
    signatures: &HashMap<String, FunctionSignature>,
    known_structs: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
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
                known_structs,
                struct_signatures,
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
    known_structs: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
    diagnostics: &mut Diagnostics,
    source_path: &Path,
) -> TypedStmt {
    match stmt {
        HirStmt::Return(expr, _) => TypedStmt::Return(lower_expr(
            expr,
            scopes,
            signatures,
            known_structs,
            struct_signatures,
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
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
            let declared_type =
                resolve_type_annotation(ty.as_ref(), known_structs, source_path, diagnostics);
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
                known_structs,
                struct_signatures,
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
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
            ensure_bool_condition(&condition, diagnostics, source_path);
            let then_body = lower_block(
                then_body,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
                diagnostics,
                true,
                source_path,
            );
            let else_body = lower_block(
                else_body,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
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
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
            ensure_bool_condition(&condition, diagnostics, source_path);
            let body = lower_block(
                body,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
                diagnostics,
                true,
                source_path,
            );
            TypedStmt::While { condition, body }
        }
        HirStmt::Expr(expr, _) => TypedStmt::Expr(lower_expr(
            expr,
            scopes,
            signatures,
            known_structs,
            struct_signatures,
            diagnostics,
            source_path,
        )),
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
    known_structs: &HashSet<String>,
    struct_signatures: &HashMap<String, StructSignature>,
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
                        known_structs,
                        struct_signatures,
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
                        known_structs,
                        struct_signatures,
                        diagnostics,
                        source_path,
                    )
                })
                .collect::<Vec<_>>();
            let call_kind = resolve_call_kind(callee, signatures, struct_signatures);
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
            let return_type = call_return_type(callee, call_kind, signatures, struct_signatures);

            TypedExpr {
                kind: match call_kind {
                    CallKind::Function | CallKind::BuiltinLen | CallKind::Unknown => {
                        TypedExprKind::Call {
                            callee: callee.clone(),
                            args: typed_args,
                        }
                    }
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
            let target = lower_expr(
                target,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
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
        HirExpr::Index {
            target,
            index,
            span,
        } => {
            let target = lower_expr(
                target,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
            let index = lower_expr(
                index,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
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
                            known_structs,
                            struct_signatures,
                            diagnostics,
                            source_path,
                        )
                    })
                    .collect::<Vec<_>>();
                let call_kind = resolve_call_kind(callee, signatures, struct_signatures);
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
                let return_type =
                    call_return_type(callee, call_kind, signatures, struct_signatures);

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
                    known_structs,
                    struct_signatures,
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
                known_structs,
                struct_signatures,
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
        HirExpr::Binary { lhs, op, rhs, span } => {
            let lhs = lower_expr(
                lhs,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
            let rhs = lower_expr(
                rhs,
                scopes,
                signatures,
                known_structs,
                struct_signatures,
                diagnostics,
                source_path,
            );
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
        CallKind::Unknown => diagnostics.push(
            Diagnostic::error(
                "GOF3004",
                format!("unknown function or struct `{callee}`"),
                "calls currently resolve only to top-level functions, builtin `len`, or struct constructors",
                span,
            )
            .with_fix_it("define the function or struct before calling it")
            .with_source_path(source_path.to_path_buf()),
        ),
    }
}

fn resolve_type_annotation(
    ty: Option<&HirTypeRef>,
    known_structs: &HashSet<String>,
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
        "task" => Type::task(Type::Unknown),
        "unit" => Type::Unit,
        name if known_structs.contains(name) => Type::Struct(name.to_string()),
        _ => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3012",
                    format!("unknown type annotation `{}`", ty.name),
                    "the bootstrap type system currently supports builtin annotations plus known struct names from the loaded module graph",
                    ty.span,
                )
                .with_fix_it("replace the annotation with a supported builtin type or a known struct name")
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
) -> CallKind {
    if callee == "len" {
        CallKind::BuiltinLen
    } else if signatures.contains_key(callee) {
        CallKind::Function
    } else if struct_signatures.contains_key(callee) {
        CallKind::Struct
    } else {
        CallKind::Unknown
    }
}

fn call_return_type(
    callee: &str,
    call_kind: CallKind,
    signatures: &HashMap<String, FunctionSignature>,
    _struct_signatures: &HashMap<String, StructSignature>,
) -> Type {
    match call_kind {
        CallKind::BuiltinLen => Type::Int,
        CallKind::Function => signatures
            .get(callee)
            .map(|signature| signature.return_type.clone())
            .unwrap_or(Type::Unknown),
        CallKind::Struct => Type::Struct(callee.to_string()),
        CallKind::Unknown => Type::Unknown,
    }
}

fn infer_binary_type(lhs: &Type, op: BinaryOp, rhs: &Type) -> Type {
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
        (Type::List(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::List(rhs))
            if types_compatible(lhs, rhs) =>
        {
            Type::Bool
        }
        (Type::Struct(lhs), BinaryOp::Eq | BinaryOp::Ne, Type::Struct(rhs)) if lhs == rhs => {
            Type::Bool
        }
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

    let element_type = match &target.ty {
        Type::List(inner) => inner.as_ref().clone(),
        Type::Unknown => Type::Unknown,
        other => {
            diagnostics.push(
                Diagnostic::error(
                    "GOF3018",
                    "indexing requires a list value",
                    format!("this target resolves to `{}`", other.display_name()),
                    span,
                )
                .with_fix_it("index only list literals or list bindings")
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
    if !matches!(arg.ty, Type::List(_) | Type::String | Type::Unknown) {
        diagnostics.push(
            Diagnostic::error(
                "GOF3019",
                "`len` requires a list or string value",
                format!("this argument resolves to `{}`", arg.ty.display_name()),
                arg.span,
            )
            .with_fix_it("pass a list literal, list binding, or string value to `len`")
            .with_source_path(source_path.to_path_buf()),
        );
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
}
