use crate::ast::BinaryOp;
use crate::typed_hir::{Type, TypedExpr, TypedExprKind, TypedFunction, TypedModule, TypedStmt};
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
    BuildList {
        dest: usize,
        items: Vec<usize>,
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
        name: function.name.clone(),
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
}
