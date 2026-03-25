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
    LoadLocal {
        dest: usize,
        name: String,
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
    Binary {
        dest: usize,
        lhs: usize,
        op: BinaryOp,
        rhs: usize,
    },
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

    for stmt in &function.body {
        match stmt {
            TypedStmt::Return(expr) => {
                let value = builder.lower_expr(expr);
                builder.instructions.push(MirInstruction::Return { value });
            }
            TypedStmt::Bind {
                name,
                mutable,
                value,
            } => {
                let src = builder.lower_expr(value);
                builder.instructions.push(MirInstruction::StoreLocal {
                    name: name.clone(),
                    src,
                    mutable: *mutable,
                    declare: true,
                });
            }
            TypedStmt::Assign { name, value } => {
                let src = builder.lower_expr(value);
                builder.instructions.push(MirInstruction::StoreLocal {
                    name: name.clone(),
                    src,
                    mutable: true,
                    declare: false,
                });
            }
            TypedStmt::Expr(expr) => {
                let value = builder.lower_expr(expr);
                builder.instructions.push(MirInstruction::Eval { value });
            }
        }
    }

    MirFunction {
        name: function.name.clone(),
        params: function.params.clone(),
        return_type: function.return_type,
        instructions: builder.instructions,
    }
}

#[derive(Default)]
struct MirBuilder {
    next_temp: usize,
    instructions: Vec<MirInstruction>,
}

impl MirBuilder {
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
            TypedExprKind::Local(name) => {
                let dest = self.alloc();
                self.instructions.push(MirInstruction::LoadLocal {
                    dest,
                    name: name.clone(),
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
