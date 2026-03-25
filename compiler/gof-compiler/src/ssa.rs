use crate::ast::{BinaryOp, UnaryOp};
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
    ConstEnumVariant {
        enum_name: String,
        variant: String,
    },
    BuildList(Vec<String>),
    BuildStruct {
        name: String,
        fields: Vec<String>,
    },
    LoadLocal(String),
    LoadField {
        target: String,
        field: String,
    },
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
    Index {
        target: String,
        index: String,
    },
    Spawn {
        callee: String,
        args: Vec<String>,
    },
    Await {
        task: String,
    },
    Unary {
        op: UnaryOp,
        value: String,
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
    BeginMatch {
        value: String,
    },
    MatchArm {
        pattern: String,
    },
    EndMatch,
    BeginSelect,
    SelectArm {
        operation: String,
        binding: Option<String>,
    },
    EndSelect,
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
        MirInstruction::ConstEnumVariant {
            dest,
            enum_name,
            variant,
        } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::ConstEnumVariant {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
            },
        },
        MirInstruction::BuildList { dest, items } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::BuildList(
                items.iter().map(|item| format!("%{item}")).collect(),
            ),
        },
        MirInstruction::BuildStruct { dest, name, fields } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::BuildStruct {
                name: name.clone(),
                fields: fields.iter().map(|field| format!("%{field}")).collect(),
            },
        },
        MirInstruction::LoadLocal { dest, name } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::LoadLocal(name.clone()),
        },
        MirInstruction::LoadField {
            dest,
            target,
            field,
        } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::LoadField {
                target: format!("%{target}"),
                field: field.clone(),
            },
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
        MirInstruction::Index {
            dest,
            target,
            index,
        } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Index {
                target: format!("%{target}"),
                index: format!("%{index}"),
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
        MirInstruction::Unary { dest, op, value } => SsaValue {
            name: format!("%{dest}"),
            instruction: SsaInstruction::Unary {
                op: *op,
                value: format!("%{value}"),
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
        MirInstruction::BeginMatch { value } => SsaValue {
            name: "%match".to_string(),
            instruction: SsaInstruction::BeginMatch {
                value: format!("%{value}"),
            },
        },
        MirInstruction::MatchArm { pattern } => SsaValue {
            name: format!("%match_arm_{pattern}"),
            instruction: SsaInstruction::MatchArm {
                pattern: format!("%{pattern}"),
            },
        },
        MirInstruction::EndMatch => SsaValue {
            name: "%endmatch".to_string(),
            instruction: SsaInstruction::EndMatch,
        },
        MirInstruction::BeginSelect => SsaValue {
            name: "%select".to_string(),
            instruction: SsaInstruction::BeginSelect,
        },
        MirInstruction::SelectArm { operation, binding } => SsaValue {
            name: format!("%select_arm_{operation}"),
            instruction: SsaInstruction::SelectArm {
                operation: format!("%{operation}"),
                binding: binding.clone(),
            },
        },
        MirInstruction::EndSelect => SsaValue {
            name: "%endselect".to_string(),
            instruction: SsaInstruction::EndSelect,
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
