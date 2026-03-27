use crate::ast::{BinaryOp, UnaryOp};
use crate::typed_hir::{
    Type, TypedExpr, TypedExprKind, TypedFunction, TypedMatchPattern, TypedModule,
    TypedSelectArmKind, TypedStmt,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Type,
    pub instructions: Vec<MirInstruction>,
}

#[derive(Debug, Clone, Serialize)]
pub enum MirInstruction {
    ConstInt {
        dest: usize,
        value: i64,
    },
    ConstString {
        dest: usize,
        value: String,
    },
    ConstBool {
        dest: usize,
        value: bool,
    },
    ConstEnumVariant {
        dest: usize,
        enum_name: String,
        variant: String,
        args: Vec<usize>,
    },
    BuildList {
        dest: usize,
        items: Vec<usize>,
    },
    BuildDict {
        dest: usize,
        entries: Vec<(usize, usize)>,
    },
    BuildStruct {
        dest: usize,
        name: String,
        fields: Vec<usize>,
    },
    LoadLocal {
        dest: usize,
        name: String,
    },
    LoadField {
        dest: usize,
        target: usize,
        field: String,
    },
    StoreLocal {
        name: String,
        src: usize,
        mutable: bool,
        declare: bool,
    },
    Break,
    Continue,
    Call {
        dest: usize,
        callee: String,
        args: Vec<usize>,
    },
    Index {
        dest: usize,
        target: usize,
        index: usize,
    },
    Spawn {
        dest: usize,
        callee: String,
        args: Vec<usize>,
    },
    Await {
        dest: usize,
        task: usize,
    },
    Propagate {
        dest: usize,
        value: usize,
    },
    Unary {
        dest: usize,
        op: UnaryOp,
        value: usize,
    },
    Binary {
        dest: usize,
        lhs: usize,
        op: BinaryOp,
        rhs: usize,
    },
    BeginIf {
        condition: usize,
    },
    Else,
    EndIf,
    BeginWhile {
        condition: usize,
    },
    EndWhile,
    BeginFor {
        iterable: usize,
        binding: String,
    },
    EndFor,
    BeginMatch {
        value: usize,
    },
    MatchArm {
        enum_name: String,
        variant: String,
        bindings: Vec<String>,
    },
    EndMatch,
    BeginSelect,
    SelectArm {
        operation: Option<usize>,
        binding: Option<String>,
        is_default: bool,
    },
    EndSelect,
    Eval {
        value: usize,
    },
    Return {
        value: usize,
    },
}

pub fn lower(module: &TypedModule) -> MirModule {
    MirModule {
        functions: module.functions.iter().map(lower_function).collect(),
    }
}

fn lower_function(function: &TypedFunction) -> MirFunction {
    let mut builder = MirBuilder::default();
    builder.lower_block(&function.body);

    MirFunction {
        name: function.symbol_name.clone(),
        params: function
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        return_type: function.return_type.clone(),
        instructions: builder.instructions,
    }
}

#[derive(Default)]
struct MirBuilder {
    next_temp: usize,
    instructions: Vec<MirInstruction>,
}

impl MirBuilder {
    fn lower_block(&mut self, body: &[TypedStmt]) {
        for stmt in body {
            self.lower_stmt(stmt);
        }
    }

    fn lower_stmt(&mut self, stmt: &TypedStmt) {
        match stmt {
            TypedStmt::Return(expr) => {
                let value = self.lower_expr(expr);
                self.instructions.push(MirInstruction::Return { value });
            }
            TypedStmt::Bind {
                name,
                mutable,
                value,
            } => {
                let src = self.lower_expr(value);
                self.instructions.push(MirInstruction::StoreLocal {
                    name: name.clone(),
                    src,
                    mutable: *mutable,
                    declare: true,
                });
            }
            TypedStmt::Assign { name, value } => {
                let src = self.lower_expr(value);
                self.instructions.push(MirInstruction::StoreLocal {
                    name: name.clone(),
                    src,
                    mutable: true,
                    declare: false,
                });
            }
            TypedStmt::Break => {
                self.instructions.push(MirInstruction::Break);
            }
            TypedStmt::Continue => {
                self.instructions.push(MirInstruction::Continue);
            }
            TypedStmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.lower_expr(condition);
                self.instructions
                    .push(MirInstruction::BeginIf { condition });
                self.lower_block(then_body);
                if !else_body.is_empty() {
                    self.instructions.push(MirInstruction::Else);
                    self.lower_block(else_body);
                }
                self.instructions.push(MirInstruction::EndIf);
            }
            TypedStmt::While { condition, body } => {
                let condition = self.lower_expr(condition);
                self.instructions
                    .push(MirInstruction::BeginWhile { condition });
                self.lower_block(body);
                self.instructions.push(MirInstruction::EndWhile);
            }
            TypedStmt::For {
                binding,
                iterable,
                body,
            } => {
                let iterable = self.lower_expr(iterable);
                self.instructions.push(MirInstruction::BeginFor {
                    iterable,
                    binding: binding.clone(),
                });
                self.lower_block(body);
                self.instructions.push(MirInstruction::EndFor);
            }
            TypedStmt::Match { value, arms } => {
                let value = self.lower_expr(value);
                self.instructions.push(MirInstruction::BeginMatch { value });
                for arm in arms {
                    let pattern = self.lower_match_pattern(&arm.pattern);
                    self.instructions.push(pattern);
                    self.lower_block(&arm.body);
                }
                self.instructions.push(MirInstruction::EndMatch);
            }
            TypedStmt::Select { arms } => {
                self.instructions.push(MirInstruction::BeginSelect);
                for arm in arms {
                    match &arm.kind {
                        TypedSelectArmKind::Recv { operation } => {
                            let operation = self.lower_expr(operation);
                            self.instructions.push(MirInstruction::SelectArm {
                                operation: Some(operation),
                                binding: arm.binding.clone(),
                                is_default: false,
                            });
                            if let Some(binding) = &arm.binding {
                                self.instructions.push(MirInstruction::StoreLocal {
                                    name: binding.clone(),
                                    src: operation,
                                    mutable: false,
                                    declare: true,
                                });
                            }
                        }
                        TypedSelectArmKind::Send { operation } => {
                            let operation = self.lower_expr(operation);
                            self.instructions.push(MirInstruction::SelectArm {
                                operation: Some(operation),
                                binding: arm.binding.clone(),
                                is_default: false,
                            });
                            if let Some(binding) = &arm.binding {
                                self.instructions.push(MirInstruction::StoreLocal {
                                    name: binding.clone(),
                                    src: operation,
                                    mutable: false,
                                    declare: true,
                                });
                            }
                        }
                        TypedSelectArmKind::Default => {
                            self.instructions.push(MirInstruction::SelectArm {
                                operation: None,
                                binding: None,
                                is_default: true,
                            });
                        }
                    }
                    self.lower_block(&arm.body);
                }
                self.instructions.push(MirInstruction::EndSelect);
            }
            TypedStmt::Expr(expr) => {
                let value = self.lower_expr(expr);
                self.instructions.push(MirInstruction::Eval { value });
            }
        }
    }

    fn lower_expr(&mut self, expr: &TypedExpr) -> usize {
        match &expr.kind {
            TypedExprKind::Int(value) => {
                let dest = self.alloc();
                self.instructions.push(MirInstruction::ConstInt {
                    dest,
                    value: *value,
                });
                dest
            }
            TypedExprKind::String(value) => {
                let dest = self.alloc();
                self.instructions.push(MirInstruction::ConstString {
                    dest,
                    value: value.clone(),
                });
                dest
            }
            TypedExprKind::Bool(value) => {
                let dest = self.alloc();
                self.instructions.push(MirInstruction::ConstBool {
                    dest,
                    value: *value,
                });
                dest
            }
            TypedExprKind::EnumVariant {
                enum_name,
                variant,
                args,
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions.push(MirInstruction::ConstEnumVariant {
                    dest,
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                    args,
                });
                dest
            }
            TypedExprKind::List { items } => {
                let items = items
                    .iter()
                    .map(|item| self.lower_expr(item))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions
                    .push(MirInstruction::BuildList { dest, items });
                dest
            }
            TypedExprKind::Dict { entries } => {
                let entries = entries
                    .iter()
                    .map(|entry| (self.lower_expr(&entry.key), self.lower_expr(&entry.value)))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions
                    .push(MirInstruction::BuildDict { dest, entries });
                dest
            }
            TypedExprKind::StructInit { name, args } => {
                let fields = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions.push(MirInstruction::BuildStruct {
                    dest,
                    name: name.clone(),
                    fields,
                });
                dest
            }
            TypedExprKind::Local(name) => {
                let dest = self.alloc();
                self.instructions.push(MirInstruction::LoadLocal {
                    dest,
                    name: name.clone(),
                });
                dest
            }
            TypedExprKind::Field { target, field } => {
                let target = self.lower_expr(target);
                let dest = self.alloc();
                self.instructions.push(MirInstruction::LoadField {
                    dest,
                    target,
                    field: field.clone(),
                });
                dest
            }
            TypedExprKind::Call { callee, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Call {
                    dest,
                    callee: callee.clone(),
                    args,
                });
                dest
            }
            TypedExprKind::MethodCall {
                target,
                symbol_name,
                args,
                ..
            } => {
                let mut call_args = vec![self.lower_expr(target)];
                call_args.extend(args.iter().map(|arg| self.lower_expr(arg)));
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Call {
                    dest,
                    callee: symbol_name.clone(),
                    args: call_args,
                });
                dest
            }
            TypedExprKind::Index { target, index } => {
                let target = self.lower_expr(target);
                let index = self.lower_expr(index);
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Index {
                    dest,
                    target,
                    index,
                });
                dest
            }
            TypedExprKind::Spawn { callee, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect::<Vec<_>>();
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Spawn {
                    dest,
                    callee: callee.clone(),
                    args,
                });
                dest
            }
            TypedExprKind::Await { value } => {
                let task = self.lower_expr(value);
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Await { dest, task });
                dest
            }
            TypedExprKind::Propagate { value } => {
                let value = self.lower_expr(value);
                let dest = self.alloc();
                self.instructions
                    .push(MirInstruction::Propagate { dest, value });
                dest
            }
            TypedExprKind::Unary { op, value } => {
                let value = self.lower_expr(value);
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Unary {
                    dest,
                    op: *op,
                    value,
                });
                dest
            }
            TypedExprKind::Binary { lhs, op, rhs } => {
                let lhs = self.lower_expr(lhs);
                let rhs = self.lower_expr(rhs);
                let dest = self.alloc();
                self.instructions.push(MirInstruction::Binary {
                    dest,
                    lhs,
                    op: *op,
                    rhs,
                });
                dest
            }
        }
    }

    fn alloc(&mut self) -> usize {
        let current = self.next_temp;
        self.next_temp += 1;
        current
    }

    fn lower_match_pattern(&mut self, pattern: &TypedMatchPattern) -> MirInstruction {
        match pattern {
            TypedMatchPattern::EnumVariant {
                enum_name,
                variant,
                bindings,
                ..
            } => MirInstruction::MatchArm {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
                bindings: bindings
                    .iter()
                    .map(|binding| binding.name.clone())
                    .collect(),
            },
        }
    }
}
