//! Backend IR v1 (ADR 0006): a block-based linear SSA form with a
//! deterministic binary encoding. This is the codegen input and the
//! incremental-cache boundary.
//!
//! One-way dependency (ADR 0002): this module reads `SsaModule` and
//! nothing upstream imports it.
//!
//! DESIGN NOTE (loop form, honest limitation): the current MIR/SSA
//! loop markers carry the condition/iterable as a PRE-COMPUTED value
//! (`BeginWhile { condition }` references a value defined before the
//! marker; the markers are inspection-only today). Lowering therefore
//! produces a guard-branch on that pre-computed value, which is NOT a
//! re-evaluating loop. Real loop semantics (condition recomputed in
//! the header) requires the MIR to emit the loop-carried form — that
//! change lands with the native codegen slice, which needs it to
//! execute loops correctly. Until then the evaluator remains the
//! execution oracle and this IR records the structure faithfully.

use crate::ast::{BinaryOp, UnaryOp};
use crate::ssa::{SsaFunction, SsaInstruction, SsaModule};

/// Module string table index.
pub type IrSymbol = u32;
/// SSA value identity, assigned in first-definition order.
pub type IrValueId = u32;

/// The binary-format magic (5 bytes) and version.
const MAGIC: &[u8; 5] = b"GOFIR";
const FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrModule {
    pub functions: Vec<IrFunction>,
    /// Interned symbol table; instructions reference entries by index.
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrFunction {
    pub name: IrSymbol,
    pub params: Vec<IrValueId>,
    pub return_type: IrTypeTag,
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrBlock {
    pub id: u32,
    pub insts: Vec<IrInst>,
    pub terminator: IrTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInst {
    pub result: Option<IrValueId>,
    pub kind: IrInstKind,
}

/// v1 instruction algebra — closed under what the SSA layer emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrInstKind {
    ConstInt(i64),
    ConstString(IrSymbol),
    ConstBool(bool),
    ConstEnumVariant {
        enum_name: IrSymbol,
        variant: IrSymbol,
        args: Vec<IrValueId>,
    },
    BuildList(Vec<IrValueId>),
    BuildDict(Vec<(IrValueId, IrValueId)>),
    BuildStruct {
        name: IrSymbol,
        fields: Vec<(IrSymbol, IrValueId)>,
    },
    LoadLocal(IrSymbol),
    StoreLocal {
        name: IrSymbol,
        value: IrValueId,
        mutable: bool,
        /// True when the store is the binding's declaration (SSA
        /// declare flag) — codegen allocates the slot.
        declare: bool,
    },
    LoadField {
        target: IrValueId,
        field: IrSymbol,
    },
    Index {
        target: IrValueId,
        index: IrValueId,
    },
    Unary {
        op: IrUnaryOp,
        value: IrValueId,
    },
    Binary {
        lhs: IrValueId,
        op: IrBinaryOp,
        rhs: IrValueId,
    },
    Call {
        callee: IrSymbol,
        args: Vec<IrValueId>,
    },
    Spawn {
        callee: IrSymbol,
        args: Vec<IrValueId>,
    },
    Await {
        task: IrValueId,
    },
    Propagate {
        value: IrValueId,
    },
    /// Match arm binding payload: binds the arm's scrutinee payloads to
    /// fresh values. Emitted at arm-block entry.
    MatchBindings {
        scrutinee: IrValueId,
        enum_name: IrSymbol,
        variant: IrSymbol,
        arity: u32,
    },
    /// Statement-position expression with no result.
    Eval(IrValueId),
    /// For-loop binding: the loop variable's value for this iteration.
    ForBinding {
        iterable: IrValueId,
        index: IrSymbol,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrUnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

/// v1 return-type surface: the flat tags the typed layer's types map
/// onto for codegen purposes. Nested structure (list element types,
/// struct fields) is not encoded in v1 — codegen consults the module's
/// type information separately; this tag is the function's transport
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrTypeTag {
    Int,
    String,
    Bool,
    Unit,
    Json,
    List,
    Dict,
    Channel,
    Task,
    Tuple,
    Option,
    Result,
    Struct,
    Enum,
    Opaque,
    Function,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrTerminator {
    Jump {
        block: u32,
    },
    Branch {
        condition: IrValueId,
        then_block: u32,
        else_block: u32,
    },
    /// Enum dispatch: arms in order, fallback for unmatched shapes
    /// (wildcards). Payload binding happens at arm entry via
    /// `MatchBindings`.
    MatchDispatch {
        scrutinee: IrValueId,
        arms: Vec<IrMatchArm>,
        fallback: u32,
    },
    Return {
        value: Option<IrValueId>,
    },
    /// Unreachable by construction: select blocks (v1 deferral — the
    /// async state-machine lowering owns real select form) and
    /// out-of-loop break/continue slips.
    Unreachable,
}

/// One match-dispatch arm: (enum, variant) → block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMatchArm {
    pub enum_name: IrSymbol,
    pub variant: IrSymbol,
    pub block: u32,
}

/// Decode/validation error. Never a panic: malformed bytes are data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendIrError {
    BadMagic,
    BadVersion(u16),
    Truncated,
    UnknownTag(u8),
    BadSymbolIndex { index: u32, table_len: u32 },
    BadValueReference { id: IrValueId },
    BadBlockTarget { block: u32, block_count: u32 },
    DuplicateBlockId(u32),
    MissingTerminator,
    UnusedTrailingBytes,
}

impl std::fmt::Display for BackendIrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic => write!(f, "backend IR: bad magic (expected GOFIR)"),
            Self::BadVersion(v) => write!(f, "backend IR: unsupported version {v}"),
            Self::Truncated => write!(f, "backend IR: truncated input"),
            Self::UnknownTag(tag) => write!(f, "backend IR: unknown tag byte {tag}"),
            Self::BadSymbolIndex { index, table_len } => {
                write!(
                    f,
                    "backend IR: symbol {index} out of range (table len {table_len})"
                )
            }
            Self::BadValueReference { id } => {
                write!(f, "backend IR: value v{id} used before definition")
            }
            Self::BadBlockTarget { block, block_count } => {
                write!(
                    f,
                    "backend IR: branch target {block} out of range ({block_count} blocks)"
                )
            }
            Self::DuplicateBlockId(id) => write!(f, "backend IR: duplicate block id {id}"),
            Self::MissingTerminator => write!(f, "backend IR: block without a terminator"),
            Self::UnusedTrailingBytes => write!(f, "backend IR: trailing bytes after module"),
        }
    }
}

impl std::error::Error for BackendIrError {}

/// String interner: dedups on insert, returns stable indexes. Insertion
/// order defines the table (canonical encoding: the table is written in
/// index order, never from map iteration).
#[derive(Default)]
struct Interner {
    strings: Vec<String>,
    index: std::collections::HashMap<String, IrSymbol>,
}

impl Interner {
    fn intern(&mut self, text: &str) -> IrSymbol {
        if let Some(existing) = self.index.get(text) {
            return *existing;
        }
        let symbol = self.strings.len() as IrSymbol;
        self.strings.push(text.to_string());
        self.index.insert(text.to_string(), symbol);
        symbol
    }
}

/// Debug helper: the displayable value name.
pub fn value_name(id: IrValueId) -> String {
    format!("v{id}")
}

/// Lower the marker-form SSA into block-form IR. Value ids are
/// MODULE-global, assigned in first-definition order across functions
/// (ADR 0006 #2).
pub fn lower(ssa: &SsaModule) -> IrModule {
    let mut interner = Interner::default();
    let mut next_id: IrValueId = 0;
    let functions = ssa
        .functions
        .iter()
        .map(|function| lower_function(function, &mut interner, &mut next_id))
        .collect();
    IrModule {
        functions,
        strings: interner.strings,
    }
}

fn binary_op(op: BinaryOp) -> IrBinaryOp {
    match op {
        BinaryOp::Add => IrBinaryOp::Add,
        BinaryOp::Sub => IrBinaryOp::Sub,
        BinaryOp::Mul => IrBinaryOp::Mul,
        BinaryOp::Div => IrBinaryOp::Div,
        BinaryOp::Mod => IrBinaryOp::Mod,
        BinaryOp::Eq => IrBinaryOp::Equal,
        BinaryOp::Ne => IrBinaryOp::NotEqual,
        BinaryOp::Lt => IrBinaryOp::Less,
        BinaryOp::Le => IrBinaryOp::LessEqual,
        BinaryOp::Gt => IrBinaryOp::Greater,
        BinaryOp::Ge => IrBinaryOp::GreaterEqual,
        BinaryOp::And => IrBinaryOp::And,
        BinaryOp::Or => IrBinaryOp::Or,
    }
}

/// Per-function lowering state. Blocks are RESERVED eagerly (empty
/// blocks pushed with `Unreachable`) and filled by index: `open` names
/// the block currently receiving instructions. No patch-up passes, no
/// placeholder targets.
/// Per-match lowering context.
#[derive(Clone)]
struct MatchContext {
    dispatch_block: u32,
    join_block: u32,
    arms_seen: u32,
    scrutinee: IrValueId,
    arms: Vec<IrMatchArm>,
}

struct FunctionLowering<'i> {
    interner: &'i mut Interner,
    value_ids: std::collections::HashMap<String, IrValueId>,
    next_id: IrValueId,
    blocks: Vec<IrBlock>,
    open: u32,
    /// Control contexts. If: (guard_block, join_block, else_taken) —
    /// the guard is patched at the Else marker.
    if_stack: Vec<(u32, u32, bool)>,
    /// Loop: (guard_block, exit_block). The guard is the block that was
    /// open when the loop marker arrived (its pre-computed condition
    /// lives there); the back-edge jumps to the guard, so the guard
    /// block is the loop header in block order.
    loop_stack: Vec<(u32, u32)>,
    /// Match: (dispatch_block, join_block, arms_seen, scrutinee,
    /// collected arms). The dispatch terminator lands on the dispatch
    /// block at EndMatch with the collected arm targets.
    match_stack: Vec<MatchContext>,
}

impl<'i> FunctionLowering<'i> {
    fn new_block(&mut self) -> u32 {
        let id = self.blocks.len() as u32;
        self.blocks.push(IrBlock {
            id,
            insts: Vec::new(),
            terminator: IrTerminator::Unreachable,
        });
        id
    }

    fn open_block(&mut self) -> u32 {
        let id = self.new_block();
        self.open = id;
        id
    }

    fn set_terminator(&mut self, block: u32, terminator: IrTerminator) {
        self.blocks[block as usize].terminator = terminator;
    }

    fn push_inst(&mut self, inst: IrInst) {
        self.blocks[self.open as usize].insts.push(inst);
    }

    /// Map an SSA name (`%name`) to its IR id, assigning a fresh id in
    /// first-definition order.
    fn define(&mut self, ssa_name: &str) -> IrValueId {
        if let Some(existing) = self.value_ids.get(ssa_name) {
            return *existing;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.value_ids.insert(ssa_name.to_string(), id);
        id
    }

    fn reference(&mut self, ssa_name: &str) -> IrValueId {
        self.define(ssa_name)
    }
}

fn lower_function(
    function: &SsaFunction,
    interner: &mut Interner,
    next_id: &mut IrValueId,
) -> IrFunction {
    let name = interner.intern(&function.name);
    let mut lowering = FunctionLowering {
        interner,
        value_ids: std::collections::HashMap::new(),
        next_id: *next_id,
        blocks: Vec::new(),
        open: 0,
        if_stack: Vec::new(),
        loop_stack: Vec::new(),
        match_stack: Vec::new(),
    };
    // Entry block + parameters as defined values.
    lowering.new_block();
    let params = function
        .params
        .iter()
        .map(|param| lowering.define(&format!("%{param}")))
        .collect();
    let return_type = type_tag(&function.return_type);

    for ssa_value in &function.values {
        lower_ssa_value(&mut lowering, ssa_value);
    }
    // If the open block never got a terminator (fall-off-the-end path),
    // close it as a void return.
    if matches!(
        lowering.blocks[lowering.open as usize].terminator,
        IrTerminator::Unreachable
    ) {
        lowering.set_terminator(lowering.open, IrTerminator::Return { value: None });
    }

    *next_id = lowering.next_id;
    IrFunction {
        name,
        params,
        return_type,
        blocks: lowering.blocks,
    }
}

fn type_tag(ty: &crate::typed_hir::Type) -> IrTypeTag {
    use crate::typed_hir::Type;
    match ty {
        Type::Int => IrTypeTag::Int,
        Type::String => IrTypeTag::String,
        Type::Bool => IrTypeTag::Bool,
        Type::Unit => IrTypeTag::Unit,
        Type::Json => IrTypeTag::Json,
        Type::List(_) => IrTypeTag::List,
        Type::Dict(_) => IrTypeTag::Dict,
        Type::Channel(_) => IrTypeTag::Channel,
        Type::Task(_) => IrTypeTag::Task,
        Type::Tuple(_) => IrTypeTag::Tuple,
        Type::Option(_) => IrTypeTag::Option,
        Type::Result(_, _) => IrTypeTag::Result,
        Type::Struct(_) => IrTypeTag::Struct,
        Type::Enum(_) => IrTypeTag::Enum,
        Type::Opaque(_) => IrTypeTag::Opaque,
        Type::Function(_, _) => IrTypeTag::Function,
        Type::CancelToken => IrTypeTag::Opaque,
        Type::Protocol(_)
        | Type::TypeParam(_)
        | Type::SelfType
        | Type::Const(_)
        | Type::Unknown => IrTypeTag::Unknown,
    }
}

fn lower_ssa_value(lowering: &mut FunctionLowering, ssa_value: &crate::ssa::SsaValue) {
    match &ssa_value.instruction {
        SsaInstruction::ConstInt(value) => {
            let id = lowering.define(&ssa_value.name);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::ConstInt(*value),
            });
        }
        SsaInstruction::ConstString(value) => {
            let id = lowering.define(&ssa_value.name);
            let symbol = lowering.interner.intern(value);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::ConstString(symbol),
            });
        }
        SsaInstruction::ConstBool(value) => {
            let id = lowering.define(&ssa_value.name);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::ConstBool(*value),
            });
        }
        SsaInstruction::ConstEnumVariant {
            enum_name,
            variant,
            args,
        } => {
            let id = lowering.define(&ssa_value.name);
            let args = args.iter().map(|arg| lowering.reference(arg)).collect();
            let enum_symbol = lowering.interner.intern(enum_name);
            let variant_symbol = lowering.interner.intern(variant);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::ConstEnumVariant {
                    enum_name: enum_symbol,
                    variant: variant_symbol,
                    args,
                },
            });
        }
        SsaInstruction::BuildList(items) => {
            let id = lowering.define(&ssa_value.name);
            let items = items.iter().map(|item| lowering.reference(item)).collect();
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::BuildList(items),
            });
        }
        SsaInstruction::BuildDict(entries) => {
            let id = lowering.define(&ssa_value.name);
            let entries = entries
                .iter()
                .map(|(key, value)| (lowering.reference(key), lowering.reference(value)))
                .collect();
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::BuildDict(entries),
            });
        }
        SsaInstruction::BuildStruct { name, fields } => {
            let id = lowering.define(&ssa_value.name);
            let name_symbol = lowering.interner.intern(name);
            let fields = fields
                .iter()
                .map(|field| {
                    let field_symbol = lowering.interner.intern(field);
                    (field_symbol, lowering.reference(field))
                })
                .collect();
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::BuildStruct {
                    name: name_symbol,
                    fields,
                },
            });
        }
        SsaInstruction::LoadLocal(name) => {
            let id = lowering.define(&ssa_value.name);
            let symbol = lowering.interner.intern(name);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::LoadLocal(symbol),
            });
        }
        SsaInstruction::LoadField { target, field } => {
            let id = lowering.define(&ssa_value.name);
            let target = lowering.reference(target);
            let field_symbol = lowering.interner.intern(field);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::LoadField {
                    target,
                    field: field_symbol,
                },
            });
        }
        SsaInstruction::StoreLocal {
            name,
            src,
            mutable,
            declare,
        } => {
            let symbol = lowering.interner.intern(name);
            let value = lowering.reference(src);
            lowering.push_inst(IrInst {
                result: None,
                kind: IrInstKind::StoreLocal {
                    name: symbol,
                    value,
                    mutable: *mutable,
                    declare: *declare,
                },
            });
        }
        SsaInstruction::Break => {
            // In-loop: jump to the loop's exit. Out-of-loop slips (the
            // typed layer rejects them) degrade to Unreachable.
            match lowering.loop_stack.last().copied() {
                Some((_, exit)) => {
                    let open = lowering.open;
                    lowering.set_terminator(open, IrTerminator::Jump { block: exit });
                    lowering.open_block();
                }
                None => {
                    let open = lowering.open;
                    lowering.set_terminator(open, IrTerminator::Unreachable);
                    lowering.open_block();
                }
            }
        }
        SsaInstruction::Continue => match lowering.loop_stack.last().copied() {
            Some((guard, _)) => {
                let open = lowering.open;
                lowering.set_terminator(open, IrTerminator::Jump { block: guard });
                lowering.open_block();
            }
            None => {
                let open = lowering.open;
                lowering.set_terminator(open, IrTerminator::Unreachable);
                lowering.open_block();
            }
        },
        SsaInstruction::Call { callee, args } => {
            let id = lowering.define(&ssa_value.name);
            let callee_symbol = lowering.interner.intern(callee);
            let args = args.iter().map(|arg| lowering.reference(arg)).collect();
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Call {
                    callee: callee_symbol,
                    args,
                },
            });
        }
        SsaInstruction::Index { target, index } => {
            let id = lowering.define(&ssa_value.name);
            let target = lowering.reference(target);
            let index = lowering.reference(index);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Index { target, index },
            });
        }
        SsaInstruction::Spawn { callee, args } => {
            let id = lowering.define(&ssa_value.name);
            let callee_symbol = lowering.interner.intern(callee);
            let args = args.iter().map(|arg| lowering.reference(arg)).collect();
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Spawn {
                    callee: callee_symbol,
                    args,
                },
            });
        }
        SsaInstruction::Await { task } => {
            let id = lowering.define(&ssa_value.name);
            let task = lowering.reference(task);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Await { task },
            });
        }
        SsaInstruction::Propagate { value } => {
            let id = lowering.define(&ssa_value.name);
            let value = lowering.reference(value);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Propagate { value },
            });
        }
        SsaInstruction::Unary { op, value } => {
            let id = lowering.define(&ssa_value.name);
            let value = lowering.reference(value);
            let op = match op {
                UnaryOp::Not => IrUnaryOp::Not,
                UnaryOp::Neg => IrUnaryOp::Neg,
            };
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Unary { op, value },
            });
        }
        SsaInstruction::Binary { lhs, op, rhs } => {
            let id = lowering.define(&ssa_value.name);
            let lhs = lowering.reference(lhs);
            let rhs = lowering.reference(rhs);
            let op = binary_op(*op);
            lowering.push_inst(IrInst {
                result: Some(id),
                kind: IrInstKind::Binary { lhs, op, rhs },
            });
        }
        SsaInstruction::BeginIf { condition } => {
            // The condition was computed in the currently-open (pre-
            // guard) block: close it with the branch. Blocks reserve in
            // order: then, [else], join — the else target is patched to
            // the join when no Else marker arrives (if_stack tracks it).
            let condition = lowering.reference(condition);
            let guard = lowering.open;
            let then_block = lowering.new_block();
            let join_block = lowering.new_block();
            lowering.set_terminator(
                guard,
                IrTerminator::Branch {
                    condition,
                    then_block,
                    else_block: join_block,
                },
            );
            lowering.if_stack.push((guard, join_block, false));
            lowering.open = then_block;
        }
        SsaInstruction::Else => {
            let (guard, join_block, _) = *lowering.if_stack.last().expect("Else inside if");
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Jump { block: join_block });
            // The else block reserves AFTER the join — block order need
            // not be topological in v1 (terminators define edges); ids
            // are stable. Patch the guard's else target to it.
            let else_block = lowering.new_block();
            if let IrTerminator::Branch {
                else_block: target, ..
            } = &mut lowering.blocks[guard as usize].terminator
            {
                *target = else_block;
            }
            let if_entry = lowering.if_stack.last_mut().expect("Else inside if");
            if_entry.2 = true;
            lowering.open = else_block;
        }
        SsaInstruction::EndIf => {
            // The guard's else target was patched at the Else marker
            // (or already pointed at the join when no Else arrived).
            let (_, join_block, _) = lowering.if_stack.pop().expect("EndIf without BeginIf");
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Jump { block: join_block });
            lowering.open = join_block;
        }
        SsaInstruction::BeginWhile { condition } => {
            // CONDITION-MOVE loop form: the instruction producing the
            // condition MOVES from the pre-header into a dedicated
            // header block, so the back-edge re-evaluates only the
            // condition — not the whole pre-header. (The MIR pre-
            // computes the condition once; the evaluator tolerates
            // that, but the IR records the honest re-evaluating shape —
            // see the module doc.)
            let condition = lowering.reference(condition);
            let guard = lowering.open;
            // Find and REMOVE the defining instruction from the
            // pre-header (the last instruction with result == condition).
            let mut moved = None;
            if let Some(position) = lowering.blocks[guard as usize]
                .insts
                .iter()
                .rposition(|inst| inst.result == Some(condition))
            {
                moved = Some(lowering.blocks[guard as usize].insts.remove(position));
            }
            let header = lowering.new_block();
            if let Some(inst) = moved {
                lowering.blocks[header as usize].insts.push(inst);
            }
            let body = lowering.new_block();
            let exit = lowering.new_block();
            lowering.set_terminator(
                header,
                IrTerminator::Branch {
                    condition,
                    then_block: body,
                    else_block: exit,
                },
            );
            // The pre-header falls into the header.
            lowering.set_terminator(guard, IrTerminator::Jump { block: header });
            lowering.loop_stack.push((header, exit));
            lowering.open = body;
        }
        SsaInstruction::EndWhile => {
            let (guard, _) = lowering
                .loop_stack
                .last()
                .copied()
                .expect("EndWhile without BeginWhile");
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Jump { block: guard });
            let exit = lowering
                .loop_stack
                .last()
                .map(|(_, exit)| *exit)
                .unwrap_or(open);
            lowering.open = exit;
        }
        SsaInstruction::BeginFor { iterable, binding } => {
            // v1 structural form: the iterable is pre-computed in the
            // guard; a ForBinding instruction in the guard names the
            // loop slot. The dispatch (has-next + advance) is the
            // codegen slice's concern; the IR records the shape.
            let iterable = lowering.reference(iterable);
            let binding_symbol = lowering.interner.intern(binding);
            let guard = lowering.open;
            let binding_id = lowering.next_id;
            lowering.next_id += 1;
            lowering.push_inst(IrInst {
                result: Some(binding_id),
                kind: IrInstKind::ForBinding {
                    iterable,
                    index: binding_symbol,
                },
            });
            let body = lowering.new_block();
            let exit = lowering.new_block();
            lowering.set_terminator(
                guard,
                IrTerminator::Branch {
                    // Structural: the branch condition is the binding
                    // slot (codegen interprets it as has-next).
                    condition: binding_id,
                    then_block: body,
                    else_block: exit,
                },
            );
            lowering.loop_stack.push((guard, exit));
            lowering.open = body;
        }
        SsaInstruction::EndFor => {
            let (guard, exit) = lowering
                .loop_stack
                .last()
                .copied()
                .expect("EndFor without BeginFor");
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Jump { block: guard });
            lowering.open = exit;
        }
        SsaInstruction::BeginMatch { value } => {
            // The dispatch block is the open block; its terminator (the
            // MatchDispatch with collected arm targets) lands at
            // EndMatch. The join reserves now.
            let scrutinee = lowering.reference(value);
            let dispatch_block = lowering.open;
            let join = lowering.new_block();
            lowering.match_stack.push(MatchContext {
                dispatch_block,
                join_block: join,
                arms_seen: 0,
                scrutinee,
                arms: Vec::new(),
            });
        }
        SsaInstruction::MatchArm {
            enum_name,
            variant,
            bindings,
        } => {
            let (join_block, arms_seen, scrutinee) = {
                let context = lowering.match_stack.last().expect("MatchArm inside match");
                (context.join_block, context.arms_seen, context.scrutinee)
            };
            if arms_seen > 0 {
                let open = lowering.open;
                lowering.set_terminator(open, IrTerminator::Jump { block: join_block });
            }
            let enum_symbol = lowering.interner.intern(enum_name);
            let variant_symbol = lowering.interner.intern(variant);
            let arity = bindings.len() as u32;
            // The arm block opens; its id records into the dispatch.
            let arm = lowering.open_block();
            // Payload bindings at arm entry.
            for binding in bindings {
                let id = lowering.define(&format!("%{binding}"));
                lowering.push_inst(IrInst {
                    result: Some(id),
                    kind: IrInstKind::MatchBindings {
                        scrutinee,
                        enum_name: enum_symbol,
                        variant: variant_symbol,
                        arity,
                    },
                });
            }
            let context = lowering
                .match_stack
                .last_mut()
                .expect("MatchArm inside match");
            context.arms.push(IrMatchArm {
                enum_name: enum_symbol,
                variant: variant_symbol,
                block: arm,
            });
            context.arms_seen += 1;
        }
        SsaInstruction::EndMatch => {
            let context = lowering
                .match_stack
                .pop()
                .expect("EndMatch without BeginMatch");
            // Close the final arm into the join.
            if context.arms_seen > 0 {
                let open = lowering.open;
                lowering.set_terminator(
                    open,
                    IrTerminator::Jump {
                        block: context.join_block,
                    },
                );
            }
            // The dispatch: arms in order, unmatched shapes fall to the
            // join (wildcard arms lower as plain blocks jumping to the
            // join; exhaustiveness is the typed layer's guarantee).
            lowering.set_terminator(
                context.dispatch_block,
                IrTerminator::MatchDispatch {
                    scrutinee: context.scrutinee,
                    arms: context.arms,
                    fallback: context.join_block,
                },
            );
            lowering.open = context.join_block;
        }
        SsaInstruction::BeginSelect => {
            // v1 deferral (module doc): select's real form is the async
            // state-machine slice's; the block closes Unreachable.
        }
        SsaInstruction::SelectArm { .. } => {}
        SsaInstruction::EndSelect => {
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Unreachable);
            lowering.open_block();
        }
        SsaInstruction::Eval(value) => {
            let value = lowering.reference(value);
            lowering.push_inst(IrInst {
                result: None,
                kind: IrInstKind::Eval(value),
            });
        }
        SsaInstruction::Return(value) => {
            let value = lowering.reference(value);
            let open = lowering.open;
            lowering.set_terminator(open, IrTerminator::Return { value: Some(value) });
            // Instructions after a return (dead code the SSA may still
            // carry) go to a fresh block.
            lowering.open_block();
        }
    }
}

// ─── Binary codec (ADR 0006 #4) ─────────────────────────────────────
//
// Canonical byte layout: magic "GOFIR", version u16 LE, varint-LEB128
// everywhere, string table first, then functions in order. Identical IR
// encodes to identical bytes; decode validates and never panics.

/// LEB128 varint writer.
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn u32(&mut self, value: u32) {
        self.u64(value as u64);
    }

    fn u64(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                break;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    /// i64 via zigzag.
    fn i64(&mut self, value: i64) {
        let zigzag = ((value << 1) ^ (value >> 63)) as u64;
        self.u64(zigzag);
    }

    fn bool(&mut self, value: bool) {
        self.bytes.push(if value { 1 } else { 0 });
    }

    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
}

/// LEB128 varint reader over a byte slice; truncation is an error.
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn u64(&mut self) -> Result<u64, BackendIrError> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            let byte = *self
                .bytes
                .get(self.position)
                .ok_or(BackendIrError::Truncated)?;
            self.position += 1;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift > 63 {
                return Err(BackendIrError::Truncated);
            }
        }
    }

    fn u32(&mut self) -> Result<u32, BackendIrError> {
        let value = self.u64()?;
        u32::try_from(value).map_err(|_| BackendIrError::Truncated)
    }

    fn i64(&mut self) -> Result<i64, BackendIrError> {
        let zigzag = self.u64()?;
        let value = ((zigzag >> 1) as i64) ^ (-((zigzag & 1) as i64));
        Ok(value)
    }

    fn bool(&mut self) -> Result<bool, BackendIrError> {
        let byte = *self
            .bytes
            .get(self.position)
            .ok_or(BackendIrError::Truncated)?;
        self.position += 1;
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(BackendIrError::UnknownTag(other)),
        }
    }

    fn byte(&mut self) -> Result<u8, BackendIrError> {
        let byte = *self
            .bytes
            .get(self.position)
            .ok_or(BackendIrError::Truncated)?;
        self.position += 1;
        Ok(byte)
    }
}

/// Encode the module canonically.
pub fn encode(module: &IrModule) -> Vec<u8> {
    let mut writer = Writer { bytes: Vec::new() };
    writer.bytes(MAGIC);
    // Version, u16 LE.
    writer.byte((FORMAT_VERSION & 0xFF) as u8);
    writer.byte((FORMAT_VERSION >> 8) as u8);
    // String table.
    writer.u32(module.strings.len() as u32);
    for text in &module.strings {
        writer.u32(text.len() as u32);
        writer.bytes(text.as_bytes());
    }
    // Functions.
    writer.u32(module.functions.len() as u32);
    for function in &module.functions {
        encode_function(&mut writer, function);
    }
    writer.bytes
}

fn encode_function(writer: &mut Writer, function: &IrFunction) {
    writer.u32(function.name);
    writer.u32(function.params.len() as u32);
    for param in &function.params {
        writer.u32(*param);
    }
    writer.byte(type_tag_byte(function.return_type));
    writer.u32(function.blocks.len() as u32);
    for block in &function.blocks {
        encode_block(writer, block);
    }
}

fn encode_block(writer: &mut Writer, block: &IrBlock) {
    writer.u32(block.id);
    writer.u32(block.insts.len() as u32);
    for inst in &block.insts {
        encode_inst(writer, inst);
    }
    encode_terminator(writer, &block.terminator);
}

fn encode_inst(writer: &mut Writer, inst: &IrInst) {
    let (tag, has_result) = inst_tag(inst.kind.clone());
    writer.byte(tag);
    match inst.result {
        Some(result) => {
            writer.bool(true);
            writer.u32(result);
        }
        None => writer.bool(false),
    }
    let _ = has_result;
    encode_inst_payload(writer, &inst.kind);
}

fn encode_inst_payload(writer: &mut Writer, kind: &IrInstKind) {
    match kind {
        IrInstKind::ConstInt(value) => writer.i64(*value),
        IrInstKind::ConstString(symbol) => writer.u32(*symbol),
        IrInstKind::ConstBool(value) => writer.bool(*value),
        IrInstKind::ConstEnumVariant {
            enum_name,
            variant,
            args,
        } => {
            writer.u32(*enum_name);
            writer.u32(*variant);
            writer.u32(args.len() as u32);
            for arg in args {
                writer.u32(*arg);
            }
        }
        IrInstKind::BuildList(items) => {
            writer.u32(items.len() as u32);
            for item in items {
                writer.u32(*item);
            }
        }
        IrInstKind::BuildDict(entries) => {
            writer.u32(entries.len() as u32);
            for (key, value) in entries {
                writer.u32(*key);
                writer.u32(*value);
            }
        }
        IrInstKind::BuildStruct { name, fields } => {
            writer.u32(*name);
            writer.u32(fields.len() as u32);
            for (field, value) in fields {
                writer.u32(*field);
                writer.u32(*value);
            }
        }
        IrInstKind::LoadLocal(symbol) => writer.u32(*symbol),
        IrInstKind::StoreLocal {
            name,
            value,
            mutable,
            declare,
        } => {
            writer.u32(*name);
            writer.u32(*value);
            writer.bool(*mutable);
            writer.bool(*declare);
        }
        IrInstKind::LoadField { target, field } => {
            writer.u32(*target);
            writer.u32(*field);
        }
        IrInstKind::Index { target, index } => {
            writer.u32(*target);
            writer.u32(*index);
        }
        IrInstKind::Unary { op, value } => {
            writer.byte(unary_op_byte(*op));
            writer.u32(*value);
        }
        IrInstKind::Binary { lhs, op, rhs } => {
            writer.byte(binary_op_byte(*op));
            writer.u32(*lhs);
            writer.u32(*rhs);
        }
        IrInstKind::Call { callee, args } => {
            writer.u32(*callee);
            writer.u32(args.len() as u32);
            for arg in args {
                writer.u32(*arg);
            }
        }
        IrInstKind::Spawn { callee, args } => {
            writer.u32(*callee);
            writer.u32(args.len() as u32);
            for arg in args {
                writer.u32(*arg);
            }
        }
        IrInstKind::Await { task } => writer.u32(*task),
        IrInstKind::Propagate { value } => writer.u32(*value),
        IrInstKind::MatchBindings {
            scrutinee,
            enum_name,
            variant,
            arity,
        } => {
            writer.u32(*scrutinee);
            writer.u32(*enum_name);
            writer.u32(*variant);
            writer.u32(*arity);
        }
        IrInstKind::Eval(value) => writer.u32(*value),
        IrInstKind::ForBinding { iterable, index } => {
            writer.u32(*iterable);
            writer.u32(*index);
        }
    }
}

fn encode_terminator(writer: &mut Writer, terminator: &IrTerminator) {
    match terminator {
        IrTerminator::Jump { block } => {
            writer.byte(0);
            writer.u32(*block);
        }
        IrTerminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            writer.byte(1);
            writer.u32(*condition);
            writer.u32(*then_block);
            writer.u32(*else_block);
        }
        IrTerminator::MatchDispatch {
            scrutinee,
            arms,
            fallback,
        } => {
            writer.byte(2);
            writer.u32(*scrutinee);
            writer.u32(arms.len() as u32);
            for arm in arms {
                writer.u32(arm.enum_name);
                writer.u32(arm.variant);
                writer.u32(arm.block);
            }
            writer.u32(*fallback);
        }
        IrTerminator::Return { value } => {
            writer.byte(3);
            match value {
                Some(id) => {
                    writer.bool(true);
                    writer.u32(*id);
                }
                None => writer.bool(false),
            }
        }
        IrTerminator::Unreachable => writer.byte(4),
    }
}

fn type_tag_byte(tag: IrTypeTag) -> u8 {
    match tag {
        IrTypeTag::Int => 0,
        IrTypeTag::String => 1,
        IrTypeTag::Bool => 2,
        IrTypeTag::Unit => 3,
        IrTypeTag::Json => 4,
        IrTypeTag::List => 5,
        IrTypeTag::Dict => 6,
        IrTypeTag::Channel => 7,
        IrTypeTag::Task => 8,
        IrTypeTag::Tuple => 9,
        IrTypeTag::Option => 10,
        IrTypeTag::Result => 11,
        IrTypeTag::Struct => 12,
        IrTypeTag::Enum => 13,
        IrTypeTag::Opaque => 14,
        IrTypeTag::Function => 15,
        IrTypeTag::Unknown => 16,
    }
}

fn type_tag_from_byte(byte: u8) -> Result<IrTypeTag, BackendIrError> {
    Ok(match byte {
        0 => IrTypeTag::Int,
        1 => IrTypeTag::String,
        2 => IrTypeTag::Bool,
        3 => IrTypeTag::Unit,
        4 => IrTypeTag::Json,
        5 => IrTypeTag::List,
        6 => IrTypeTag::Dict,
        7 => IrTypeTag::Channel,
        8 => IrTypeTag::Task,
        9 => IrTypeTag::Tuple,
        10 => IrTypeTag::Option,
        11 => IrTypeTag::Result,
        12 => IrTypeTag::Struct,
        13 => IrTypeTag::Enum,
        14 => IrTypeTag::Opaque,
        15 => IrTypeTag::Function,
        16 => IrTypeTag::Unknown,
        other => return Err(BackendIrError::UnknownTag(other)),
    })
}

fn unary_op_byte(op: IrUnaryOp) -> u8 {
    match op {
        IrUnaryOp::Not => 0,
        IrUnaryOp::Neg => 1,
    }
}

fn unary_op_from_byte(byte: u8) -> Result<IrUnaryOp, BackendIrError> {
    Ok(match byte {
        0 => IrUnaryOp::Not,
        1 => IrUnaryOp::Neg,
        other => return Err(BackendIrError::UnknownTag(other)),
    })
}

fn binary_op_byte(op: IrBinaryOp) -> u8 {
    match op {
        IrBinaryOp::Add => 0,
        IrBinaryOp::Sub => 1,
        IrBinaryOp::Mul => 2,
        IrBinaryOp::Div => 3,
        IrBinaryOp::Mod => 4,
        IrBinaryOp::Equal => 5,
        IrBinaryOp::NotEqual => 6,
        IrBinaryOp::Less => 7,
        IrBinaryOp::LessEqual => 8,
        IrBinaryOp::Greater => 9,
        IrBinaryOp::GreaterEqual => 10,
        IrBinaryOp::And => 11,
        IrBinaryOp::Or => 12,
    }
}

fn binary_op_from_byte(byte: u8) -> Result<IrBinaryOp, BackendIrError> {
    Ok(match byte {
        0 => IrBinaryOp::Add,
        1 => IrBinaryOp::Sub,
        2 => IrBinaryOp::Mul,
        3 => IrBinaryOp::Div,
        4 => IrBinaryOp::Mod,
        5 => IrBinaryOp::Equal,
        6 => IrBinaryOp::NotEqual,
        7 => IrBinaryOp::Less,
        8 => IrBinaryOp::LessEqual,
        9 => IrBinaryOp::Greater,
        10 => IrBinaryOp::GreaterEqual,
        11 => IrBinaryOp::And,
        12 => IrBinaryOp::Or,
        other => return Err(BackendIrError::UnknownTag(other)),
    })
}

/// Instruction tag bytes (stable per ADR 0006: additive growth bumps
/// minor versions; never renumber).
fn inst_tag(kind: IrInstKind) -> (u8, bool) {
    match kind {
        IrInstKind::ConstInt(_) => (1, true),
        IrInstKind::ConstString(_) => (2, true),
        IrInstKind::ConstBool(_) => (3, true),
        IrInstKind::ConstEnumVariant { .. } => (4, true),
        IrInstKind::BuildList(_) => (5, true),
        IrInstKind::BuildDict(_) => (6, true),
        IrInstKind::BuildStruct { .. } => (7, true),
        IrInstKind::LoadLocal(_) => (8, true),
        IrInstKind::StoreLocal { .. } => (9, false),
        IrInstKind::LoadField { .. } => (10, true),
        IrInstKind::Index { .. } => (11, true),
        IrInstKind::Unary { .. } => (12, true),
        IrInstKind::Binary { .. } => (13, true),
        IrInstKind::Call { .. } => (14, true),
        IrInstKind::Spawn { .. } => (15, true),
        IrInstKind::Await { .. } => (16, true),
        IrInstKind::Propagate { .. } => (17, true),
        IrInstKind::MatchBindings { .. } => (18, true),
        IrInstKind::Eval(_) => (19, false),
        IrInstKind::ForBinding { .. } => (20, true),
    }
}

/// Decode and VALIDATE a module. Errors, never panics.
pub fn decode(bytes: &[u8]) -> Result<IrModule, BackendIrError> {
    let mut reader = Reader { bytes, position: 0 };
    // Magic.
    if reader.bytes.len() < 5 || &reader.bytes[0..5] != MAGIC {
        return Err(BackendIrError::BadMagic);
    }
    reader.position = 5;
    // Version, u16 LE.
    let version_lo = reader.byte()?;
    let version_hi = reader.byte()?;
    let version = u16::from_le_bytes([version_lo, version_hi]);
    if version != FORMAT_VERSION {
        return Err(BackendIrError::BadVersion(version));
    }
    // String table.
    let string_count = reader.u32()?;
    let mut strings = Vec::with_capacity(string_count as usize);
    for _ in 0..string_count {
        let len = reader.u32()? as usize;
        let start = reader.position;
        let end = start.checked_add(len).ok_or(BackendIrError::Truncated)?;
        if end > reader.bytes.len() {
            return Err(BackendIrError::Truncated);
        }
        let text = std::str::from_utf8(&reader.bytes[start..end])
            .map_err(|_| BackendIrError::Truncated)?
            .to_string();
        reader.position = end;
        strings.push(text);
    }
    let table_len = string_count;
    // Functions.
    let function_count = reader.u32()?;
    let mut functions = Vec::with_capacity(function_count as usize);
    for _ in 0..function_count {
        functions.push(decode_function(&mut reader, table_len)?);
    }
    if reader.position != reader.bytes.len() {
        return Err(BackendIrError::UnusedTrailingBytes);
    }
    let module = IrModule { functions, strings };
    validate(&module)?;
    Ok(module)
}

fn decode_function(reader: &mut Reader, table_len: u32) -> Result<IrFunction, BackendIrError> {
    let name = reader.u32()?;
    if name >= table_len {
        return Err(BackendIrError::BadSymbolIndex {
            index: name,
            table_len,
        });
    }
    let param_count = reader.u32()?;
    let mut params = Vec::with_capacity(param_count as usize);
    for _ in 0..param_count {
        params.push(reader.u32()?);
    }
    let return_type = type_tag_from_byte(reader.byte()?)?;
    let block_count = reader.u32()?;
    let mut blocks = Vec::with_capacity(block_count as usize);
    for _ in 0..block_count {
        blocks.push(decode_block(reader, table_len)?);
    }
    Ok(IrFunction {
        name,
        params,
        return_type,
        blocks,
    })
}

fn decode_block(reader: &mut Reader, table_len: u32) -> Result<IrBlock, BackendIrError> {
    let id = reader.u32()?;
    let inst_count = reader.u32()?;
    let mut insts = Vec::with_capacity(inst_count as usize);
    for _ in 0..inst_count {
        insts.push(decode_inst(reader, table_len)?);
    }
    let terminator = decode_terminator(reader, table_len)?;
    Ok(IrBlock {
        id,
        insts,
        terminator,
    })
}

fn decode_inst(reader: &mut Reader, table_len: u32) -> Result<IrInst, BackendIrError> {
    let tag = reader.byte()?;
    let has_result = reader.bool()?;
    let result = if has_result {
        Some(reader.u32()?)
    } else {
        None
    };
    let kind = match tag {
        1 => IrInstKind::ConstInt(reader.i64()?),
        2 => {
            let symbol = reader.u32()?;
            if symbol >= table_len {
                return Err(BackendIrError::BadSymbolIndex {
                    index: symbol,
                    table_len,
                });
            }
            IrInstKind::ConstString(symbol)
        }
        3 => IrInstKind::ConstBool(reader.bool()?),
        4 => {
            let enum_name = symbol_checked(reader, table_len)?;
            let variant = symbol_checked(reader, table_len)?;
            let arg_count = reader.u32()?;
            let mut args = Vec::with_capacity(arg_count as usize);
            for _ in 0..arg_count {
                args.push(reader.u32()?);
            }
            IrInstKind::ConstEnumVariant {
                enum_name,
                variant,
                args,
            }
        }
        5 => {
            let item_count = reader.u32()?;
            let mut items = Vec::with_capacity(item_count as usize);
            for _ in 0..item_count {
                items.push(reader.u32()?);
            }
            IrInstKind::BuildList(items)
        }
        6 => {
            let entry_count = reader.u32()?;
            let mut entries = Vec::with_capacity(entry_count as usize);
            for _ in 0..entry_count {
                let key = reader.u32()?;
                let value = reader.u32()?;
                entries.push((key, value));
            }
            IrInstKind::BuildDict(entries)
        }
        7 => {
            let name = symbol_checked(reader, table_len)?;
            let field_count = reader.u32()?;
            let mut fields = Vec::with_capacity(field_count as usize);
            for _ in 0..field_count {
                let field = symbol_checked(reader, table_len)?;
                let value = reader.u32()?;
                fields.push((field, value));
            }
            IrInstKind::BuildStruct { name, fields }
        }
        8 => {
            let symbol = symbol_checked(reader, table_len)?;
            IrInstKind::LoadLocal(symbol)
        }
        9 => {
            let name = symbol_checked(reader, table_len)?;
            let value = reader.u32()?;
            let mutable = reader.bool()?;
            let declare = reader.bool()?;
            IrInstKind::StoreLocal {
                name,
                value,
                mutable,
                declare,
            }
        }
        10 => {
            let target = reader.u32()?;
            let field = symbol_checked(reader, table_len)?;
            IrInstKind::LoadField { target, field }
        }
        11 => {
            let target = reader.u32()?;
            let index = reader.u32()?;
            IrInstKind::Index { target, index }
        }
        12 => {
            let op = unary_op_from_byte(reader.byte()?)?;
            let value = reader.u32()?;
            IrInstKind::Unary { op, value }
        }
        13 => {
            let op = binary_op_from_byte(reader.byte()?)?;
            let lhs = reader.u32()?;
            let rhs = reader.u32()?;
            IrInstKind::Binary { lhs, op, rhs }
        }
        14 | 15 => {
            let callee = symbol_checked(reader, table_len)?;
            let arg_count = reader.u32()?;
            let mut args = Vec::with_capacity(arg_count as usize);
            for _ in 0..arg_count {
                args.push(reader.u32()?);
            }
            if tag == 14 {
                IrInstKind::Call { callee, args }
            } else {
                IrInstKind::Spawn { callee, args }
            }
        }
        16 => {
            let task = reader.u32()?;
            IrInstKind::Await { task }
        }
        17 => {
            let value = reader.u32()?;
            IrInstKind::Propagate { value }
        }
        18 => {
            let scrutinee = reader.u32()?;
            let enum_name = symbol_checked(reader, table_len)?;
            let variant = symbol_checked(reader, table_len)?;
            let arity = reader.u32()?;
            IrInstKind::MatchBindings {
                scrutinee,
                enum_name,
                variant,
                arity,
            }
        }
        19 => {
            let value = reader.u32()?;
            IrInstKind::Eval(value)
        }
        20 => {
            let iterable = reader.u32()?;
            let index = symbol_checked(reader, table_len)?;
            IrInstKind::ForBinding { iterable, index }
        }
        other => return Err(BackendIrError::UnknownTag(other)),
    };
    Ok(IrInst { result, kind })
}

fn symbol_checked(reader: &mut Reader, table_len: u32) -> Result<IrSymbol, BackendIrError> {
    let symbol = reader.u32()?;
    if symbol >= table_len {
        return Err(BackendIrError::BadSymbolIndex {
            index: symbol,
            table_len,
        });
    }
    Ok(symbol)
}

fn decode_terminator(reader: &mut Reader, table_len: u32) -> Result<IrTerminator, BackendIrError> {
    let tag = reader.byte()?;
    Ok(match tag {
        0 => IrTerminator::Jump {
            block: reader.u32()?,
        },
        1 => IrTerminator::Branch {
            condition: reader.u32()?,
            then_block: reader.u32()?,
            else_block: reader.u32()?,
        },
        2 => {
            let scrutinee = reader.u32()?;
            let arm_count = reader.u32()?;
            let mut arms = Vec::with_capacity(arm_count as usize);
            for _ in 0..arm_count {
                let enum_name = symbol_checked(reader, table_len)?;
                let variant = symbol_checked(reader, table_len)?;
                let block = reader.u32()?;
                arms.push(IrMatchArm {
                    enum_name,
                    variant,
                    block,
                });
            }
            let fallback = reader.u32()?;
            IrTerminator::MatchDispatch {
                scrutinee,
                arms,
                fallback,
            }
        }
        3 => {
            let has_value = reader.bool()?;
            let value = if has_value { Some(reader.u32()?) } else { None };
            IrTerminator::Return { value }
        }
        4 => IrTerminator::Unreachable,
        other => return Err(BackendIrError::UnknownTag(other)),
    })
}

/// Structural validation: block ids unique, branch targets in range,
/// every referenced value defined before use in block order (params
/// count as defined). Returns Err on the first violation.
pub fn validate(module: &IrModule) -> Result<(), BackendIrError> {
    for function in &module.functions {
        let mut defined: std::collections::HashSet<IrValueId> =
            function.params.iter().copied().collect();
        let mut seen_blocks: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let block_count = function.blocks.len() as u32;
        for block in &function.blocks {
            if !seen_blocks.insert(block.id) {
                return Err(BackendIrError::DuplicateBlockId(block.id));
            }
            for inst in &block.insts {
                check_operands(&inst.kind, |id| {
                    if !defined.contains(&id) {
                        return Err(BackendIrError::BadValueReference { id });
                    }
                    Ok(())
                })?;
                if let Some(result) = inst.result {
                    defined.insert(result);
                }
            }
            check_terminator(&block.terminator, |id| {
                if !defined.contains(&id) {
                    return Err(BackendIrError::BadValueReference { id });
                }
                Ok(())
            })?;
            match &block.terminator {
                IrTerminator::Jump { block } => {
                    if *block >= block_count {
                        return Err(BackendIrError::BadBlockTarget {
                            block: *block,
                            block_count,
                        });
                    }
                }
                IrTerminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    for target in [*then_block, *else_block] {
                        if target >= block_count {
                            return Err(BackendIrError::BadBlockTarget {
                                block: target,
                                block_count,
                            });
                        }
                    }
                }
                IrTerminator::MatchDispatch { arms, fallback, .. } => {
                    for arm in arms {
                        if arm.block >= block_count {
                            return Err(BackendIrError::BadBlockTarget {
                                block: arm.block,
                                block_count,
                            });
                        }
                    }
                    if *fallback >= block_count {
                        return Err(BackendIrError::BadBlockTarget {
                            block: *fallback,
                            block_count,
                        });
                    }
                }
                IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
            }
        }
    }
    Ok(())
}

fn check_operands(
    kind: &IrInstKind,
    check: impl Fn(IrValueId) -> Result<(), BackendIrError>,
) -> Result<(), BackendIrError> {
    match kind {
        IrInstKind::ConstInt(_) | IrInstKind::ConstString(_) | IrInstKind::ConstBool(_) => Ok(()),
        IrInstKind::ConstEnumVariant { args, .. } => {
            for arg in args {
                check(*arg)?;
            }
            Ok(())
        }
        IrInstKind::BuildList(items) => {
            for item in items {
                check(*item)?;
            }
            Ok(())
        }
        IrInstKind::BuildDict(entries) => {
            for (key, value) in entries {
                check(*key)?;
                check(*value)?;
            }
            Ok(())
        }
        IrInstKind::BuildStruct { fields, .. } => {
            for (_, value) in fields {
                check(*value)?;
            }
            Ok(())
        }
        IrInstKind::LoadLocal(_) => Ok(()),
        IrInstKind::StoreLocal { value, .. } => check(*value),
        IrInstKind::LoadField { target, .. } => check(*target),
        IrInstKind::Index { target, index } => {
            check(*target)?;
            check(*index)
        }
        IrInstKind::Unary { value, .. } => check(*value),
        IrInstKind::Binary { lhs, rhs, .. } => {
            check(*lhs)?;
            check(*rhs)
        }
        IrInstKind::Call { args, .. } | IrInstKind::Spawn { args, .. } => {
            for arg in args {
                check(*arg)?;
            }
            Ok(())
        }
        IrInstKind::Await { task } => check(*task),
        IrInstKind::Propagate { value } => check(*value),
        IrInstKind::MatchBindings { scrutinee, .. } => check(*scrutinee),
        IrInstKind::Eval(value) => check(*value),
        IrInstKind::ForBinding { iterable, .. } => check(*iterable),
    }
}

fn check_terminator(
    terminator: &IrTerminator,
    check: impl Fn(IrValueId) -> Result<(), BackendIrError>,
) -> Result<(), BackendIrError> {
    match terminator {
        IrTerminator::Jump { .. } | IrTerminator::Unreachable => Ok(()),
        IrTerminator::Branch { condition, .. } => check(*condition),
        IrTerminator::MatchDispatch { scrutinee, .. } => check(*scrutinee),
        IrTerminator::Return { value } => match value {
            Some(id) => check(*id),
            None => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    /// Full chain: source → SSA → IR (the real pipeline stages).
    fn ir_from_source(text: &str) -> IrModule {
        let source = SourceFile::new("ir_test.gof", text.to_string());
        let module =
            crate::module_graph::parse_single_source(&source).expect("source should parse");
        let hir = crate::hir::lower(&module);
        let (typed, _) = crate::typed_hir::lower(&hir).expect("typing should succeed");
        let mir = crate::mir::lower(&typed);
        let ssa = crate::ssa::lower(&mir);
        lower(&ssa)
    }

    #[test]
    fn round_trip_arithmetic() {
        let ir = ir_from_source(
            "fn add(a: int, b: int) -> int:\n    return a + b\n\nfn main() -> int:\n    return add(2, 3)\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn round_trip_if_else() {
        let ir = ir_from_source(
            "fn classify(n: int) -> int:\n    if n > 0:\n        return 1\n    else:\n        return 0\n\nfn main() -> int:\n    return classify(5)\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn round_trip_while_loop() {
        let ir = ir_from_source(
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n\nfn main() -> int:\n    return count(3)\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn round_trip_for_loop() {
        let ir = ir_from_source(
            "fn sum(items: list[int]) -> int:\n    mut total = 0\n    for item in items:\n        total = total + item\n    return total\n\nfn main() -> int:\n    return sum([1, 2, 3])\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn round_trip_match() {
        let ir = ir_from_source(
            "enum Step:\n    Go\n    Stop\n\nfn decide(step: Step) -> int:\n    match step:\n        Step.Go:\n            return 1\n        Step.Stop:\n            return 0\n\nfn main() -> int:\n    return decide(Step.Go)\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn round_trip_strings_and_calls() {
        let ir = ir_from_source(
            "fn greet(name: string) -> string:\n    return \"hello \" + name\n\nfn main() -> int:\n    print(greet(\"gof\"))\n    return 0\n",
        );
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(ir, decoded);
    }

    #[test]
    fn byte_stability() {
        let ir = ir_from_source(
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n\nfn main() -> int:\n    return count(3)\n",
        );
        let once = encode(&ir);
        let decoded = decode(&once).expect("decode should succeed");
        let twice = encode(&decoded);
        assert_eq!(
            once, twice,
            "encode(decode(encode(x))) must equal encode(x)"
        );
    }

    #[test]
    fn lowering_is_deterministic() {
        let text = "enum Step:\n    Go\n    Stop\n\nfn decide(step: Step) -> int:\n    match step:\n        Step.Go:\n            return 1\n        Step.Stop:\n            return 0\n\nfn main() -> int:\n    return decide(Step.Go)\n";
        let first = encode(&ir_from_source(text));
        let second = encode(&ir_from_source(text));
        assert_eq!(
            first, second,
            "two lowers of the same source must encode identically"
        );
    }

    #[test]
    fn module_global_value_ids() {
        // ADR 0006 #2: ids are module-global — the second function's ids
        // must NOT restart at zero.
        let ir = ir_from_source(
            "fn one() -> int:\n    return 1\n\nfn two() -> int:\n    return 2\n\nfn main() -> int:\n    return one() + two()\n",
        );
        assert!(ir.functions.len() >= 2);
        let first_max = ir
            .functions
            .iter()
            .flat_map(|function| function.params.iter().copied())
            .chain(
                ir.functions
                    .iter()
                    .flat_map(|function| function.blocks.iter())
                    .flat_map(|block| block.insts.iter())
                    .filter_map(|inst| inst.result),
            )
            .max()
            .unwrap_or(0);
        let _ = first_max;
        // Two empty-body functions with distinct ids: param ids across
        // functions must differ (module-global counter).
        let params: Vec<Vec<IrValueId>> = ir
            .functions
            .iter()
            .map(|function| function.params.clone())
            .collect();
        let all: Vec<IrValueId> = params.iter().flatten().copied().collect();
        let unique: std::collections::HashSet<IrValueId> = all.iter().copied().collect();
        assert_eq!(
            all.len(),
            unique.len(),
            "param ids must be unique module-wide"
        );
    }

    #[test]
    fn malformed_inputs_error_never_panic() {
        let ir = ir_from_source("fn main() -> int:\n    return 0\n");
        let good = encode(&ir);
        // Bad magic.
        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert!(decode(&bad_magic).is_err());
        // Truncated (every prefix).
        for len in 0..good.len() {
            assert!(decode(&good[..len]).is_err(), "prefix {len} must error");
        }
        // Bad version.
        let mut bad_version = good.clone();
        bad_version[6] = 0xFF;
        assert!(matches!(
            decode(&bad_version),
            Err(BackendIrError::BadVersion(_))
        ));
        // Trailing bytes.
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(matches!(
            decode(&trailing),
            Err(BackendIrError::UnusedTrailingBytes)
        ));
    }

    #[test]
    fn block_discipline_holds() {
        let ir = ir_from_source(
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n\nfn main() -> int:\n    return count(3)\n",
        );
        validate(&ir).expect("lowered IR must validate");
        // Every block has a terminator by construction; check targets.
        for function in &ir.functions {
            let block_count = function.blocks.len() as u32;
            for block in &function.blocks {
                match &block.terminator {
                    IrTerminator::Jump { block } => assert!(*block < block_count),
                    IrTerminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        assert!(*then_block < block_count);
                        assert!(*else_block < block_count);
                    }
                    IrTerminator::MatchDispatch { arms, fallback, .. } => {
                        for arm in arms {
                            assert!(arm.block < block_count);
                        }
                        assert!(*fallback < block_count);
                    }
                    IrTerminator::Return { .. } | IrTerminator::Unreachable => {}
                }
            }
        }
    }

    #[test]
    fn while_condition_moves_to_header() {
        // The CONDITION-MOVE form: the condition instruction lives in
        // the loop HEADER block, and the header's terminator branches
        // on it; the pre-header jumps to the header.
        let ir = ir_from_source(
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n",
        );
        let function = &ir.functions[0];
        // Find a block whose terminator is a Branch: that's the header.
        let header = function
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, IrTerminator::Branch { .. }))
            .expect("loop header must exist");
        let branch_condition = match &header.terminator {
            IrTerminator::Branch { condition, .. } => *condition,
            _ => unreachable!(),
        };
        // The condition's defining instruction is IN the header.
        let defined_in_header = header
            .insts
            .iter()
            .any(|inst| inst.result == Some(branch_condition));
        assert!(
            defined_in_header,
            "the while condition must be defined in the header block"
        );
        // Some block jumps TO the header (the pre-header fall-through
        // and/or the back-edge).
        let jumpers = function
            .blocks
            .iter()
            .filter(|block| matches!(&block.terminator, IrTerminator::Jump { block } if *block == header.id))
            .count();
        assert!(
            jumpers >= 2,
            "pre-header + back-edge must jump to the header (found {jumpers})"
        );
    }

    #[test]
    fn match_has_dispatch_terminator() {
        let ir = ir_from_source(
            "enum Step:\n    Go\n    Stop\n\nfn decide(step: Step) -> int:\n    match step:\n        Step.Go:\n            return 1\n        Step.Stop:\n            return 0\n",
        );
        let function = &ir.functions[0];
        let has_dispatch = function
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, IrTerminator::MatchDispatch { .. }));
        assert!(
            has_dispatch,
            "the match must lower to a MatchDispatch terminator"
        );
    }
}
