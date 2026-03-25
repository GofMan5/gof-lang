use crate::ast::BinaryOp;
use crate::mir::{MirFunction, MirInstruction, MirModule};
use crate::typed_hir::Type;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SsaModule {
    pub functions: Vec<SsaFunction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SsaFunction {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Type,
    pub values: Vec<SsaValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SsaValue {
    pub name: String,
    pub instruction: SsaInstruction,
}

#[derive(Debug, Clone, Serialize)]
pub enum SsaInstruction {
    ConstInt(i64),
    ConstString(String),
    ConstBool(bool),
    LoadLocal(String),
    StoreLocal {
        name: String,
        src: String,
        mutable: bool,
        declare: bool,
    },
    Call {
        callee: String,
        args: Vec<String>,
    },
    Spawn {
        callee: String,
        args: Vec<String>,
    },
    Await {
        task: String,
    },
    Binary {
        lhs: String,
        op: BinaryOp,
        rhs: String,
    },
    BeginIf {
        condition: String,
    },
    Else,
    EndIf,
    BeginWhile {
        condition: String,
    },
    EndWhile,
    Eval(String),
    Return(String),
}

pub fn lower(module: &MirModule) -> SsaModule {
    SsaModule {
        functions: module.functions.iter().map(lower_function).collect(),
    }
}

fn lower_function(function: &MirFunction) -> SsaFunction {
    let values = function
        .instructions
        .iter()
        .map(lower_instruction)
        .collect::<Vec<_>>();

    SsaFunction {
        name: function.name.clone(),
        params: function.params.clone(),
        return_type: function.return_type.clone(),
        values,
    }
}

fn lower_instruction(instruction: &MirInstruction) -> SsaValue {
    match instruction {
        MirInstruction::ConstInt { dest, value } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::ConstInt(*value),
        },
        MirInstruction::ConstString { dest, value } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::ConstString(value.clone()),
        },
        MirInstruction::ConstBool { dest, value } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::ConstBool(*value),
        },
        MirInstruction::LoadLocal { dest, name } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::LoadLocal(name.clone()),
        },
        MirInstruction::StoreLocal {
            name,
            src,
            mutable,
            declare,
        } => SsaValue {
            name: format!("%store_{name}"),
            instruction: SsaInstruction::StoreLocal {
                name: name.clone(),
                src: format!("%{src}"),
                mutable: *mutable,
                declare: *declare,
            },
        },
        MirInstruction::Call { dest, callee, args } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Call {
                callee: callee.clone(),
                args: args.iter().map(|arg| format!("%{arg}")).collect(),
            },
        },
        MirInstruction::Spawn { dest, callee, args } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Spawn {
                callee: callee.clone(),
                args: args.iter().map(|arg| format!("%{arg}")).collect(),
            },
        },
        MirInstruction::Await { dest, task } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Await {
                task: format!("%{task}"),
            },
        },
        MirInstruction::Binary { dest, lhs, op, rhs } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Binary {
                lhs: format!("%{lhs}"),
                op: *op,
                rhs: format!("%{rhs}"),
            },
        },
        MirInstruction::BeginIf { condition } => SsaValue {
            name: "%if".to_string(),
            instruction: SsaInstruction::BeginIf {
                condition: format!("%{condition}"),
            },
        },
        MirInstruction::Else => SsaValue {
            name: "%else".to_string(),
            instruction: SsaInstruction::Else,
        },
        MirInstruction::EndIf => SsaValue {
            name: "%endif".to_string(),
            instruction: SsaInstruction::EndIf,
        },
        MirInstruction::BeginWhile { condition } => SsaValue {
            name: "%while".to_string(),
            instruction: SsaInstruction::BeginWhile {
                condition: format!("%{condition}"),
            },
        },
        MirInstruction::EndWhile => SsaValue {
            name: "%endwhile".to_string(),
            instruction: SsaInstruction::EndWhile,
        },
        MirInstruction::Eval { value } => SsaValue {
            name: format!("%eval_{value}"),
            instruction: SsaInstruction::Eval(format!("%{value}")),
        },
        MirInstruction::Return { value } => SsaValue {
            name: "%ret".to_string(),
            instruction: SsaInstruction::Return(format!("%{value}")),
        },
    }
}
