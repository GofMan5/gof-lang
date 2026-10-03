//! Backend IR v1 (ADR 0006): block-based linear SSA, lowered one-way
//! from [`crate::ssa::SsaModule`], plus the canonical binary encoding
//! that is the incremental-cache boundary.
//!
//! Layering (ADR 0002): this module is the last compiler layer — it
//! reads `ssa` (and, transitively through it, `ast` op enums and
//! `typed_hir::Type` for return-type tags). Nothing upstream imports
//! it.
//!
//! # Structural contract
//!
//! - a function is an ordered list of blocks; every block ends in
//!   exactly one terminator (`IrBlock::terminator` is not an
//!   `Option`, so the invariant is type-level for constructed IR)
//! - block ids equal block indexes (assigned in creation order); the
//!   binary format relies on this and `decode`/`validate` enforce it
//! - value ids are `u32`, assigned by a module-global counter in
//!   first-definition order; a function's params are defined before
//!   its instruction results, and every operand must reference an id
//!   defined earlier in the same function (def-before-use in block
//!   order)
//! - symbols are indexes into the module string table (interned with
//!   dedup on first use)
//!
//! # Control-flow lowering (from the marker form in `ssa.rs`)
//!
//! `mir.rs` emits each marker AFTER the instructions that compute its
//! operand: an `if`/`while`/`match` condition or `for` iterable is
//! fully evaluated before the `Begin*` marker appears in the value
//! list. The markers lower as follows:
//!
//! - `if`: the condition stays in the pre-branch block, which ends in
//!   `Branch{cond, then, else}`. `Else` switches to the else block;
//!   `EndIf` merges the arms into a lazily-created join block. With no
//!   `Else` marker the (empty) else block doubles as the join.
//! - `while`: MIR emits the condition ONCE, before `BeginWhile`, but
//!   source semantics re-evaluate it every iteration. The lowering
//!   therefore MOVES the condition's instruction run out of the
//!   pre-header into a dedicated loop header. The run is the
//!   transitive operand closure of the condition value — by MIR
//!   construction a contiguous suffix of the current block (expression
//!   lowering emits no markers). The result is the canonical
//!   non-rotated shape:
//!
//!   ```text
//!   pre-header: [pre-loop insts]      Jump -> header
//!   header:     [condition insts]     Branch{cond, body, exit}
//!   body:       [body insts]          Jump -> header   (EndWhile)
//!   exit:       [post-loop insts]
//!   ```
//!
//!   Ids and instruction order are untouched (a block split, never a
//!   re-emission), so first-definition order is preserved, and
//!   re-executing the header re-evaluates the condition — exactly
//!   `while` semantics.
//! - `for`: the iterable is evaluated once in the pre-header, which
//!   jumps to an empty loop header. FOR-LOOP V1 DEFERRAL (honest
//!   limitation, not a silent shortcut): the v1 instruction algebra
//!   has no iteration primitive — the MIR's `BeginFor` is itself a
//!   marker-interpreted construct — so the header terminates in
//!   [`IrTerminator::Unreachable`] instead of a fabricated exit test.
//!   The body's real instructions are preserved verbatim, `EndFor`
//!   emits the back-edge `Jump` to the header, and the exit block
//!   follows the region. The real header edges land with the codegen
//!   slice (index-based `while` expansion or an explicit iterator
//!   instruction — that slice's call). The loop binding is carried
//!   implicitly by the body's `LoadLocal` instructions.
//! - `match`: the scrutinee stays in the dispatch block, which ends in
//!   `MatchDispatch{scrutinee, arms, fallback}` — the `Switch`
//!   terminator from ADR 0006. Each `MatchArm` opens an arm block;
//!   guards (plain `Eval` in MIR already, per RFC 0052's note that
//!   guard dispatch is Phase 2 work) and the arm body lower into it;
//!   `EndMatch` closes every arm into a join block and points the
//!   fallback ("no arm matched") edge there too. The variant test
//!   itself is not expressible in the v1 algebra: the arm list is the
//!   contract the codegen slice lowers (tag compare + destructure),
//!   and the arm's `bindings` are carried as symbol NAMES — the
//!   destructure that would define them as values lands with that
//!   slice, so inventing value ids here would be fake.
//! - `select`: SELECT V1 DEFERRAL (honest limitation, not a silent
//!   shortcut): select lowering lands with the async state-machine
//!   work (Phase 2+); the evaluator remains the execution surface for
//!   select. The region lowers to `Unreachable`-terminated blocks:
//!   `BeginSelect` terminates the incoming block, each `SelectArm`
//!   marks an arm boundary, `EndSelect` opens the continuation block.
//!   Arm operations and binding stores around the markers are
//!   ordinary instructions and are preserved verbatim; no dispatch
//!   edges are invented.
//! - `break`/`continue`: inside a loop, `Jump` to the innermost loop's
//!   exit/header (loop stack). Outside a loop, `Unreachable` — the
//!   typed layer already rejects those, and the terminator keeps
//!   block discipline instead of silently dropping the statement.
//! - instructions that follow a terminator (e.g. code after `return`
//!   inside an arm) land in fresh unreachable blocks: every input
//!   instruction is preserved, but dead regions gain no edges.
//! - a function whose final block falls through ends in
//!   `Return{value: None}` (implicit unit return).
//!
//! # Binary format (canonical)
//!
//! `GOFIR` magic + `u16` LE version (=1), LEB128 varints throughout
//! (`i64` zigzagged): string table (count, then len + UTF-8 per
//! entry), then functions (count; per function: name symbol, param
//! count + ids, return-type tag, block count; per block: id,
//! instruction count, instructions, terminator; per instruction: tag
//! byte, result slot, payload). Identical IR encodes to identical
//! bytes: field order is fixed, there is no padding, and no `HashMap`
//! iteration order reaches the encoder (the interner map is lookup
//! only). Decoding validates magic/version, tags, ranges, UTF-8,
//! def-before-use, block id/target bounds and exact byte consumption
//! — malformed input is an `Err`, never a panic, never UB.

use crate::ast::{BinaryOp, UnaryOp};
use crate::ssa::{SsaFunction, SsaInstruction, SsaModule, SsaValue};
use crate::typed_hir::Type;
use std::collections::{HashMap, HashSet};

/// Module string table index.
pub type IrSymbol = u32;
/// SSA value identity, assigned in first-definition order by the
/// module-global counter.
pub type IrValueId = u32;

/// Debug utility: renders a value id as `v{id}` (test output, error
/// messages).
pub fn value_name(id: IrValueId) -> String {
    format!("v{id}")
}

/// Wire-format constants. The tag values are the stable binary
/// contract: additive growth appends new tags; renumbering is a
/// layout change and bumps the format version.
mod wire {
    pub const MAGIC: &[u8; 5] = b"GOFIR";
    pub const VERSION: u16 = 1;

    // instruction tags
    pub const CONST_INT: u8 = 1;
    pub const CONST_STRING: u8 = 2;
    pub const CONST_BOOL: u8 = 3;
    pub const CONST_ENUM_VARIANT: u8 = 4;
    pub const BUILD_LIST: u8 = 5;
    pub const BUILD_DICT: u8 = 6;
    pub const BUILD_STRUCT: u8 = 7;
    pub const LOAD_LOCAL: u8 = 8;
    pub const LOAD_FIELD: u8 = 9;
    pub const INDEX: u8 = 10;
    pub const UNARY: u8 = 11;
    pub const BINARY: u8 = 12;
    pub const CALL: u8 = 13;
    pub const SPAWN: u8 = 14;
    pub const AWAIT: u8 = 15;
    pub const PROPAGATE: u8 = 16;
    pub const STORE_LOCAL: u8 = 17;
    pub const EVAL: u8 = 18;

    // terminator tags
    pub const JUMP: u8 = 1;
    pub const BRANCH: u8 = 2;
    pub const MATCH_DISPATCH: u8 = 3;
    pub const RETURN: u8 = 4;
    pub const UNREACHABLE: u8 = 5;

    // return-type tags
    pub const TYPE_INT: u8 = 1;
    pub const TYPE_STRING: u8 = 2;
    pub const TYPE_BOOL: u8 = 3;
    pub const TYPE_UNIT: u8 = 4;
    pub const TYPE_JSON: u8 = 5;
    pub const TYPE_LIST: u8 = 6;
    pub const TYPE_DICT: u8 = 7;
    pub const TYPE_CHANNEL: u8 = 8;
    pub const TYPE_TASK: u8 = 9;
    pub const TYPE_OPTION: u8 = 10;
    pub const TYPE_RESULT: u8 = 11;
    pub const TYPE_STRUCT: u8 = 12;
    pub const TYPE_ENUM: u8 = 13;
    pub const TYPE_OPAQUE: u8 = 14;
    pub const TYPE_UNKNOWN: u8 = 15;

    // operator tags (tied to the #[repr(u8)] discriminants)
    pub const OP_NOT: u8 = super::IrUnaryOp::Not as u8;
    pub const OP_NEG: u8 = super::IrUnaryOp::Neg as u8;
    pub const OP_ADD: u8 = super::IrBinaryOp::Add as u8;
    pub const OP_SUB: u8 = super::IrBinaryOp::Sub as u8;
    pub const OP_MUL: u8 = super::IrBinaryOp::Mul as u8;
    pub const OP_DIV: u8 = super::IrBinaryOp::Div as u8;
    pub const OP_MOD: u8 = super::IrBinaryOp::Mod as u8;
    pub const OP_AND: u8 = super::IrBinaryOp::And as u8;
    pub const OP_OR: u8 = super::IrBinaryOp::Or as u8;
    pub const OP_EQ: u8 = super::IrBinaryOp::Eq as u8;
    pub const OP_NE: u8 = super::IrBinaryOp::Ne as u8;
    pub const OP_LT: u8 = super::IrBinaryOp::Lt as u8;
    pub const OP_LE: u8 = super::IrBinaryOp::Le as u8;
    pub const OP_GT: u8 = super::IrBinaryOp::Gt as u8;
    pub const OP_GE: u8 = super::IrBinaryOp::Ge as u8;
}

/// The compiled module: functions plus the interned string table.
///
/// The IR types deliberately do NOT derive `serde` traits: the binary
/// format in [`encode`]/[`decode`] IS the serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrModule {
    pub functions: Vec<IrFunction>,
    /// Interned symbol table; instructions reference entries by index.
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrFunction {
    /// Function name (symbol table index).
    pub name: IrSymbol,
    /// Incoming-argument value ids (defined before any instruction
    /// result; the body reads them through `LoadLocal` by name, the
    /// codegen slice binds them to argument slots).
    pub params: Vec<IrValueId>,
    pub return_type: IrTypeTag,
    /// Block id == block index (creation order); block 0 is the entry.
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrBlock {
    pub id: u32,
    pub insts: Vec<IrInst>,
    /// Every block ends in exactly one terminator.
    pub terminator: IrTerminator,
}

/// Compact return-type tag.
///
/// v1 covers the flat tags the typed layer needs for transport:
/// struct/enum/opaque constructor names land in the string table.
/// LIMITATION (v1, deliberate): parameterized constructors (`List`,
/// `Dict`, `Channel`, `Task`, `Option`, `Result`) drop their payloads
/// — `list[int]` and `list[string]` are both [`IrTypeTag::List`];
/// full type fidelity is the codegen slice's type table, not this
/// tag. `CancelToken`, `Tuple`, `Function`, `Const`, `Protocol`,
/// `TypeParam` and `SelfType` degrade to [`IrTypeTag::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
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
    Option,
    Result,
    Struct(IrSymbol),
    Enum(IrSymbol),
    Opaque(IrSymbol),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrInst {
    /// `Some` for value-producing instructions; `None` for
    /// side-effect instructions (`StoreLocal`, `Eval`).
    pub result: Option<IrValueId>,
    pub kind: IrInstKind,
}

/// v1 instruction algebra — closed under what the SSA layer emits
/// today; growth is additive (new tags, see [`wire`]).
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
    /// Struct fields are POSITIONAL (field order = the struct
    /// declaration order). The MIR does not carry field names at the
    /// construction site; fabricating them here would be fake.
    BuildStruct {
        sym: IrSymbol,
        fields: Vec<IrValueId>,
    },
    LoadLocal {
        sym: IrSymbol,
    },
    LoadField {
        value: IrValueId,
        sym: IrSymbol,
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
        op: IrBinaryOp,
        lhs: IrValueId,
        rhs: IrValueId,
    },
    Call {
        sym: IrSymbol,
        args: Vec<IrValueId>,
    },
    Spawn {
        sym: IrSymbol,
        args: Vec<IrValueId>,
    },
    Await {
        task: IrValueId,
    },
    Propagate {
        value: IrValueId,
    },
    /// Local store. A store is an instruction (side effect), never a
    /// value producer: `result` is always `None`, including plain
    /// re-assignments (`declare == false`).
    StoreLocal {
        sym: IrSymbol,
        value: IrValueId,
        mutable: bool,
        declare: bool,
    },
    /// Statement-position expression whose value is discarded.
    Eval {
        value: IrValueId,
    },
}

/// Mirror of [`crate::ast::UnaryOp`] with wire-stable discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IrUnaryOp {
    Not = 1,
    Neg = 2,
}

/// Mirror of [`crate::ast::BinaryOp`] with wire-stable discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IrBinaryOp {
    Add = 1,
    Sub = 2,
    Mul = 3,
    Div = 4,
    Mod = 5,
    And = 6,
    Or = 7,
    Eq = 8,
    Ne = 9,
    Lt = 10,
    Le = 11,
    Gt = 12,
    Ge = 13,
}

/// One arm of a match dispatch. `bindings` are the arm pattern's
/// binding NAMES (symbols): the v1 algebra has no destructure
/// instruction, so the values those locals receive are materialized by
/// the codegen slice, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrMatchArm {
    pub enum_name: IrSymbol,
    pub variant: IrSymbol,
    pub bindings: Vec<IrSymbol>,
    /// Target block for this arm (in dispatch order).
    pub block: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrTerminator {
    Jump {
        block: u32,
    },
    Branch {
        cond: IrValueId,
        then_block: u32,
        else_block: u32,
    },
    /// The ADR's `Switch`: dispatch on an enum scrutinee to arm
    /// blocks, in source order. `fallback` is the "no arm matched"
    /// edge (exhaustiveness is enforced by the typed layer; guards are
    /// not dispatched in v1 — see the match note in the module docs).
    MatchDispatch {
        scrutinee: IrValueId,
        arms: Vec<IrMatchArm>,
        fallback: u32,
    },
    Return {
        value: Option<IrValueId>,
    },
    /// No successors. Produced by: the for-loop header deferral, the
    /// select deferral, `break`/`continue` outside a loop, and
    /// unreachable regions kept for structure.
    Unreachable,
}

impl From<UnaryOp> for IrUnaryOp {
    fn from(op: UnaryOp) -> Self {
        match op {
            UnaryOp::Not => IrUnaryOp::Not,
            UnaryOp::Neg => IrUnaryOp::Neg,
        }
    }
}

impl From<BinaryOp> for IrBinaryOp {
    fn from(op: BinaryOp) -> Self {
        match op {
            BinaryOp::Add => IrBinaryOp::Add,
            BinaryOp::Sub => IrBinaryOp::Sub,
            BinaryOp::Mul => IrBinaryOp::Mul,
            BinaryOp::Div => IrBinaryOp::Div,
            BinaryOp::Mod => IrBinaryOp::Mod,
            BinaryOp::And => IrBinaryOp::And,
            BinaryOp::Or => IrBinaryOp::Or,
            BinaryOp::Eq => IrBinaryOp::Eq,
            BinaryOp::Ne => IrBinaryOp::Ne,
            BinaryOp::Lt => IrBinaryOp::Lt,
            BinaryOp::Le => IrBinaryOp::Le,
            BinaryOp::Gt => IrBinaryOp::Gt,
            BinaryOp::Ge => IrBinaryOp::Ge,
        }
    }
}

/// Decode/validation error. Malformed bytes are data, never a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendIrError {
    BadMagic,
    UnsupportedVersion(u16),
    Truncated,
    /// A varint longer than 10 bytes or overflowing `u64`.
    VarintOverflow,
    /// Bytes left over after the module decoded.
    TrailingBytes,
    /// A string table entry was not valid UTF-8.
    InvalidUtf8,
    /// Unknown instruction/terminator/type/operator tag byte.
    InvalidTag {
        context: &'static str,
        tag: u8,
    },
    /// A boolean/presence byte outside 0/1.
    InvalidByte {
        context: &'static str,
        byte: u8,
    },
    /// A list count larger than the remaining byte count.
    CountExceedsRemaining {
        context: &'static str,
    },
    /// A varint that must fit `u32` did not.
    IntegerTooLarge {
        context: &'static str,
        value: u64,
    },
    /// A symbol index at or beyond the string table length.
    SymbolOutOfRange {
        symbol: IrSymbol,
        table_len: usize,
    },
    /// An operand referencing an id not yet defined in this function
    /// (def-before-use in block order).
    ValueNotDefined {
        id: IrValueId,
        context: &'static str,
    },
    /// A value id defined twice (params and results are unique
    /// module-wide).
    ValueRedefined {
        id: IrValueId,
    },
    /// A block id that does not equal its index.
    BlockIdMismatch {
        index: usize,
        id: u32,
    },
    /// A terminator target at or beyond the function's block count.
    BlockTargetOutOfRange {
        target: u32,
        block_count: usize,
    },
    /// A side-effect instruction (`StoreLocal`/`Eval`) that binds a
    /// result value.
    SideEffectWithResult,
}

impl std::fmt::Display for BackendIrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic => write!(f, "backend IR: bad magic (expected GOFIR)"),
            Self::UnsupportedVersion(version) => {
                write!(f, "backend IR: unsupported format version {version}")
            }
            Self::Truncated => write!(f, "backend IR: truncated input"),
            Self::VarintOverflow => write!(f, "backend IR: varint overflows u64"),
            Self::TrailingBytes => write!(f, "backend IR: trailing bytes after module"),
            Self::InvalidUtf8 => write!(f, "backend IR: string table entry is not UTF-8"),
            Self::InvalidTag { context, tag } => {
                write!(f, "backend IR: unknown {context} tag byte {tag}")
            }
            Self::InvalidByte { context, byte } => {
                write!(
                    f,
                    "backend IR: invalid {context} byte {byte} (expected 0 or 1)"
                )
            }
            Self::CountExceedsRemaining { context } => {
                write!(f, "backend IR: {context} count exceeds remaining bytes")
            }
            Self::IntegerTooLarge { context, value } => {
                write!(f, "backend IR: {context} {value} does not fit u32")
            }
            Self::SymbolOutOfRange { symbol, table_len } => write!(
                f,
                "backend IR: symbol {symbol} out of range (table len {table_len})"
            ),
            Self::ValueNotDefined { id, .. } => write!(
                f,
                "backend IR: {} used before it is defined in this function",
                value_name(*id)
            ),
            Self::ValueRedefined { id } => write!(
                f,
                "backend IR: {} is defined more than once",
                value_name(*id)
            ),
            Self::BlockIdMismatch { index, id } => {
                write!(
                    f,
                    "backend IR: block at index {index} has id {id} (ids must equal indexes)"
                )
            }
            Self::BlockTargetOutOfRange {
                target,
                block_count,
            } => write!(
                f,
                "backend IR: branch target {target} out of range ({block_count} blocks)"
            ),
            Self::SideEffectWithResult => write!(
                f,
                "backend IR: StoreLocal/Eval must not bind a result value"
            ),
        }
    }
}

impl std::error::Error for BackendIrError {}

// ---------------------------------------------------------------------------
// Lowering: SsaModule -> IrModule
// ---------------------------------------------------------------------------

/// Lowers a marker-form SSA module into block-based IR v1. One-way:
/// there is no IR -> SSA path. See the module docs for the control
/// flow shapes and the documented v1 deferrals.
pub fn lower(ssa: &SsaModule) -> IrModule {
    let mut module = ModuleBuilder::default();
    let functions = ssa
        .functions
        .iter()
        .map(|function| module.lower_function(function))
        .collect::<Vec<_>>();
    IrModule {
        functions,
        strings: module.strings,
    }
}

#[derive(Default)]
struct ModuleBuilder {
    strings: Vec<String>,
    interned: HashMap<String, IrSymbol>,
    next_value: IrValueId,
}

impl ModuleBuilder {
    fn intern(&mut self, text: &str) -> IrSymbol {
        if let Some(symbol) = self.interned.get(text) {
            return *symbol;
        }
        let symbol = self.strings.len() as IrSymbol;
        self.strings.push(text.to_string());
        self.interned.insert(text.to_string(), symbol);
        symbol
    }

    fn fresh_value(&mut self) -> IrValueId {
        let id = self.next_value;
        self.next_value += 1;
        id
    }

    fn lower_function(&mut self, function: &SsaFunction) -> IrFunction {
        let name = self.intern(&function.name);
        let params = function
            .params
            .iter()
            .map(|_| self.fresh_value())
            .collect::<Vec<_>>();
        let return_type = ir_type_tag(self, &function.return_type);
        let mut builder = FnBuilder::new(self);
        for value in &function.values {
            builder.lower_value(value);
        }
        let blocks = builder.finish();
        IrFunction {
            name,
            params,
            return_type,
            blocks,
        }
    }
}

/// Maps a typed-layer return type onto the compact v1 tag. See
/// [`IrTypeTag`] for the documented fidelity limits.
fn ir_type_tag(module: &mut ModuleBuilder, ty: &Type) -> IrTypeTag {
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
        Type::Option(_) => IrTypeTag::Option,
        Type::Result(_, _) => IrTypeTag::Result,
        Type::Struct(name) => IrTypeTag::Struct(module.intern(name)),
        Type::Enum(name) => IrTypeTag::Enum(module.intern(name)),
        Type::Opaque(name) => IrTypeTag::Opaque(module.intern(name)),
        // v1 tag-surface limitation, documented on IrTypeTag: these
        // have no v1 tag and degrade to Unknown.
        Type::Unknown => IrTypeTag::Unknown,
        Type::CancelToken
        | Type::Tuple(_)
        | Type::Function(_, _)
        | Type::Const(_)
        | Type::Protocol(_)
        | Type::TypeParam(_)
        | Type::SelfType => IrTypeTag::Unknown,
    }
}

/// A block under construction: the terminator is `None` until the
/// block is closed. Every block is closed by the time `finish` runs.
struct RawBlock {
    id: u32,
    insts: Vec<IrInst>,
    term: Option<IrTerminator>,
}

struct IfFrame {
    else_block: usize,
    /// Lazily created join block (created when some edge needs it,
    /// which keeps block creation order = reading order).
    join: Option<usize>,
    saw_else: bool,
}

struct LoopFrame {
    header: usize,
    exit: usize,
}

struct MatchFrame {
    dispatch: usize,
    fallback: usize,
    join: Option<usize>,
    arms: Vec<IrMatchArm>,
    scrutinee: IrValueId,
}

struct FnBuilder<'module> {
    module: &'module mut ModuleBuilder,
    /// SSA value name -> value id (marker names and store names are
    /// never inserted; only value-producing temps are).
    names: HashMap<String, IrValueId>,
    /// result id -> operand ids, for the while-header split (the
    /// transitive operand closure of the condition).
    edges: HashMap<IrValueId, Vec<IrValueId>>,
    blocks: Vec<RawBlock>,
    current: usize,
    if_stack: Vec<IfFrame>,
    loop_stack: Vec<LoopFrame>,
    match_stack: Vec<MatchFrame>,
}

impl<'module> FnBuilder<'module> {
    fn new(module: &'module mut ModuleBuilder) -> Self {
        Self {
            module,
            names: HashMap::new(),
            edges: HashMap::new(),
            blocks: vec![RawBlock {
                id: 0,
                insts: Vec::new(),
                term: None,
            }],
            current: 0,
            if_stack: Vec::new(),
            loop_stack: Vec::new(),
            match_stack: Vec::new(),
        }
    }

    fn new_block(&mut self) -> usize {
        let id = self.blocks.len() as u32;
        self.blocks.push(RawBlock {
            id,
            insts: Vec::new(),
            term: None,
        });
        self.blocks.len() - 1
    }

    fn block_id(&self, index: usize) -> u32 {
        self.blocks[index].id
    }

    /// Opens a fresh block if the current one is already terminated
    /// (unreachable region: instructions after a `return`/`break`/...
    /// land there, preserved but edge-free).
    fn ensure_open(&mut self) {
        if self.blocks[self.current].term.is_some() {
            self.current = self.new_block();
        }
    }

    /// Terminates the current block unless it is already terminated
    /// (an arm that ended in `return`/`break` contributes no edge).
    fn close_if_open(&mut self, term: IrTerminator) {
        if self.blocks[self.current].term.is_none() {
            self.blocks[self.current].term = Some(term);
        }
    }

    /// Resolves an SSA operand name. The MIR contract guarantees
    /// def-before-use; a miss is a broken input invariant and fails
    /// loudly instead of producing garbage IR.
    fn resolve(&self, name: &str) -> IrValueId {
        self.names.get(name).copied().unwrap_or_else(|| {
            panic!("backend_ir: SSA operand `{name}` is used before it is defined")
        })
    }

    /// Emits a value-producing instruction and binds a fresh id to the
    /// SSA value name.
    fn define(&mut self, name: &str, kind: IrInstKind, operands: Vec<IrValueId>) {
        self.ensure_open();
        let id = self.module.fresh_value();
        self.names.insert(name.to_string(), id);
        self.edges.insert(id, operands);
        self.blocks[self.current].insts.push(IrInst {
            result: Some(id),
            kind,
        });
    }

    /// Emits a side-effect instruction (`StoreLocal`/`Eval`): no
    /// result, no fresh id.
    fn emit_side_effect(&mut self, kind: IrInstKind) {
        self.ensure_open();
        self.blocks[self.current]
            .insts
            .push(IrInst { result: None, kind });
    }

    fn lower_value(&mut self, value: &SsaValue) {
        match &value.instruction {
            SsaInstruction::ConstInt(int) => {
                self.define(&value.name, IrInstKind::ConstInt(*int), Vec::new());
            }
            SsaInstruction::ConstString(text) => {
                let sym = self.module.intern(text);
                self.define(&value.name, IrInstKind::ConstString(sym), Vec::new());
            }
            SsaInstruction::ConstBool(flag) => {
                self.define(&value.name, IrInstKind::ConstBool(*flag), Vec::new());
            }
            SsaInstruction::ConstEnumVariant {
                enum_name,
                variant,
                args,
            } => {
                let enum_sym = self.module.intern(enum_name);
                let variant_sym = self.module.intern(variant);
                let operands = args.iter().map(|arg| self.resolve(arg)).collect::<Vec<_>>();
                self.define(
                    &value.name,
                    IrInstKind::ConstEnumVariant {
                        enum_name: enum_sym,
                        variant: variant_sym,
                        args: operands.clone(),
                    },
                    operands,
                );
            }
            SsaInstruction::BuildList(items) => {
                let operands = items
                    .iter()
                    .map(|item| self.resolve(item))
                    .collect::<Vec<_>>();
                self.define(
                    &value.name,
                    IrInstKind::BuildList(operands.clone()),
                    operands,
                );
            }
            SsaInstruction::BuildDict(entries) => {
                let pairs = entries
                    .iter()
                    .map(|(key, val)| (self.resolve(key), self.resolve(val)))
                    .collect::<Vec<_>>();
                let operands = pairs
                    .iter()
                    .flat_map(|(key, val)| [*key, *val])
                    .collect::<Vec<_>>();
                self.define(&value.name, IrInstKind::BuildDict(pairs), operands);
            }
            SsaInstruction::BuildStruct { name, fields } => {
                let sym = self.module.intern(name);
                let operands = fields
                    .iter()
                    .map(|field| self.resolve(field))
                    .collect::<Vec<_>>();
                self.define(
                    &value.name,
                    IrInstKind::BuildStruct {
                        sym,
                        fields: operands.clone(),
                    },
                    operands,
                );
            }
            SsaInstruction::LoadLocal(local) => {
                let sym = self.module.intern(local);
                self.define(&value.name, IrInstKind::LoadLocal { sym }, Vec::new());
            }
            SsaInstruction::LoadField { target, field } => {
                let operand = self.resolve(target);
                let sym = self.module.intern(field);
                self.define(
                    &value.name,
                    IrInstKind::LoadField {
                        value: operand,
                        sym,
                    },
                    vec![operand],
                );
            }
            SsaInstruction::StoreLocal {
                name,
                src,
                mutable,
                declare,
            } => {
                let sym = self.module.intern(name);
                let operand = self.resolve(src);
                self.emit_side_effect(IrInstKind::StoreLocal {
                    sym,
                    value: operand,
                    mutable: *mutable,
                    declare: *declare,
                });
            }
            SsaInstruction::Break => self.break_stmt(),
            SsaInstruction::Continue => self.continue_stmt(),
            SsaInstruction::Call { callee, args } => {
                let sym = self.module.intern(callee);
                let operands = args.iter().map(|arg| self.resolve(arg)).collect::<Vec<_>>();
                self.define(
                    &value.name,
                    IrInstKind::Call {
                        sym,
                        args: operands.clone(),
                    },
                    operands,
                );
            }
            SsaInstruction::Index { target, index } => {
                let target_id = self.resolve(target);
                let index_id = self.resolve(index);
                self.define(
                    &value.name,
                    IrInstKind::Index {
                        target: target_id,
                        index: index_id,
                    },
                    vec![target_id, index_id],
                );
            }
            SsaInstruction::Spawn { callee, args } => {
                let sym = self.module.intern(callee);
                let operands = args.iter().map(|arg| self.resolve(arg)).collect::<Vec<_>>();
                self.define(
                    &value.name,
                    IrInstKind::Spawn {
                        sym,
                        args: operands.clone(),
                    },
                    operands,
                );
            }
            SsaInstruction::Await { task } => {
                let operand = self.resolve(task);
                self.define(
                    &value.name,
                    IrInstKind::Await { task: operand },
                    vec![operand],
                );
            }
            SsaInstruction::Propagate { value: inner } => {
                let operand = self.resolve(inner);
                self.define(
                    &value.name,
                    IrInstKind::Propagate { value: operand },
                    vec![operand],
                );
            }
            SsaInstruction::Unary { op, value: inner } => {
                let operand = self.resolve(inner);
                self.define(
                    &value.name,
                    IrInstKind::Unary {
                        op: IrUnaryOp::from(*op),
                        value: operand,
                    },
                    vec![operand],
                );
            }
            SsaInstruction::Binary { lhs, op, rhs } => {
                let lhs_id = self.resolve(lhs);
                let rhs_id = self.resolve(rhs);
                self.define(
                    &value.name,
                    IrInstKind::Binary {
                        op: IrBinaryOp::from(*op),
                        lhs: lhs_id,
                        rhs: rhs_id,
                    },
                    vec![lhs_id, rhs_id],
                );
            }
            SsaInstruction::BeginIf { condition } => self.begin_if(condition),
            SsaInstruction::Else => self.else_marker(),
            SsaInstruction::EndIf => self.end_if(),
            SsaInstruction::BeginWhile { condition } => self.begin_while(condition),
            SsaInstruction::EndWhile => self.end_while(),
            SsaInstruction::BeginFor { iterable, binding } => self.begin_for(iterable, binding),
            SsaInstruction::EndFor => self.end_for(),
            SsaInstruction::BeginMatch { value: scrutinee } => self.begin_match(scrutinee),
            SsaInstruction::MatchArm {
                enum_name,
                variant,
                bindings,
            } => self.match_arm(enum_name, variant, bindings),
            SsaInstruction::EndMatch => self.end_match(),
            SsaInstruction::BeginSelect => self.begin_select(),
            SsaInstruction::SelectArm {
                operation,
                binding,
                is_default,
            } => self.select_arm(operation.as_deref(), binding.as_deref(), *is_default),
            SsaInstruction::EndSelect => self.end_select(),
            SsaInstruction::Eval(inner) => {
                let operand = self.resolve(inner);
                self.emit_side_effect(IrInstKind::Eval { value: operand });
            }
            SsaInstruction::Return(inner) => self.return_stmt(inner),
        }
    }

    // -- if / else -----------------------------------------------------------

    fn begin_if(&mut self, condition: &str) {
        // The condition's instructions precede the marker (mir.rs
        // lowers the condition expression first), so the value is
        // already defined in the current block: branch straight from
        // it.
        let cond = self.resolve(condition);
        self.ensure_open();
        let then_block = self.new_block();
        let else_block = self.new_block();
        self.blocks[self.current].term = Some(IrTerminator::Branch {
            cond,
            then_block: self.block_id(then_block),
            else_block: self.block_id(else_block),
        });
        self.current = then_block;
        self.if_stack.push(IfFrame {
            else_block,
            join: None,
            saw_else: false,
        });
    }

    fn if_join(&mut self) -> u32 {
        let existing = self.if_stack.last().and_then(|frame| frame.join);
        match existing {
            Some(index) => self.blocks[index].id,
            None => {
                let index = self.new_block();
                let id = self.blocks[index].id;
                self.if_stack
                    .last_mut()
                    .expect("backend_ir: if join requested without an open if frame")
                    .join = Some(index);
                id
            }
        }
    }

    fn else_marker(&mut self) {
        let join = self.if_join();
        self.close_if_open(IrTerminator::Jump { block: join });
        let frame = self
            .if_stack
            .last_mut()
            .expect("backend_ir: Else marker without a matching BeginIf");
        frame.saw_else = true;
        let else_block = frame.else_block;
        self.current = else_block;
    }

    fn end_if(&mut self) {
        let frame = self
            .if_stack
            .pop()
            .expect("backend_ir: EndIf marker without a matching BeginIf");
        if !frame.saw_else {
            // No Else marker: the (empty) else block doubles as the
            // join — the then-arm falls into it and execution
            // continues there.
            let else_id = self.blocks[frame.else_block].id;
            self.close_if_open(IrTerminator::Jump { block: else_id });
            self.current = frame.else_block;
            return;
        }
        let join_index = frame.join.unwrap_or_else(|| self.new_block());
        let join_id = self.blocks[join_index].id;
        self.close_if_open(IrTerminator::Jump { block: join_id });
        self.current = join_index;
    }

    // -- while ---------------------------------------------------------------

    /// The transitive operand closure of a value, over the recorded
    /// instruction edges. Deterministic: the set is only queried by
    /// membership, never iterated.
    fn value_closure(&self, root: IrValueId) -> HashSet<IrValueId> {
        let mut seen = HashSet::new();
        let mut work = vec![root];
        while let Some(id) = work.pop() {
            if seen.insert(id) {
                if let Some(operands) = self.edges.get(&id) {
                    work.extend(operands.iter().copied());
                }
            }
        }
        seen
    }

    fn begin_while(&mut self, condition: &str) {
        let cond = self.resolve(condition);
        let closure = self.value_closure(cond);
        if self.blocks[self.current].term.is_none() {
            // Open pre-header: split off the condition run (the
            // contiguous trailing instructions whose results are in
            // the closure) and move it into the header, so the
            // condition is re-evaluated every iteration. See the
            // WHILE note in the module docs.
            let split = {
                let insts = &self.blocks[self.current].insts;
                let mut split = insts.len();
                while split > 0 {
                    match insts[split - 1].result {
                        Some(id) if closure.contains(&id) => split -= 1,
                        _ => break,
                    }
                }
                split
            };
            let moved: Vec<IrInst> = self.blocks[self.current].insts.drain(split..).collect();
            let header = self.new_block();
            self.blocks[header].insts = moved;
            let body = self.new_block();
            let exit = self.new_block();
            self.blocks[self.current].term = Some(IrTerminator::Jump {
                block: self.block_id(header),
            });
            self.blocks[header].term = Some(IrTerminator::Branch {
                cond,
                then_block: self.block_id(body),
                else_block: self.block_id(exit),
            });
            self.current = body;
            self.loop_stack.push(LoopFrame { header, exit });
            return;
        }
        // Defensive path (unreachable for MIR-shaped input: the
        // condition instructions directly precede the marker, so the
        // current block is open): the loop region is unreachable from
        // here and the header is only reachable via the back-edge.
        let header = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.blocks[header].term = Some(IrTerminator::Branch {
            cond,
            then_block: self.block_id(body),
            else_block: self.block_id(exit),
        });
        self.current = body;
        self.loop_stack.push(LoopFrame { header, exit });
    }

    fn end_while(&mut self) {
        let frame = self
            .loop_stack
            .pop()
            .expect("backend_ir: EndWhile marker without a matching BeginWhile");
        let header_id = self.blocks[frame.header].id;
        self.close_if_open(IrTerminator::Jump { block: header_id });
        self.current = frame.exit;
    }

    // -- for -----------------------------------------------------------------

    fn begin_for(&mut self, iterable: &str, _binding: &str) {
        // Resolving validates the operand contract; the iterable's
        // instructions are already in the current (pre-header) block.
        // The binding name is not represented in v1 IR: the body's
        // LoadLocal instructions reference it by symbol, and the
        // iteration primitive that would assign it lands with the
        // codegen slice (FOR-LOOP V1 DEFERRAL — see module docs).
        let _iterable = self.resolve(iterable);
        let header = self.new_block();
        let body = self.new_block();
        let exit = self.new_block();
        self.close_if_open(IrTerminator::Jump {
            block: self.block_id(header),
        });
        // The header's real terminator (iterate-or-exit test) is not
        // expressible in the v1 instruction algebra. Unreachable is
        // the honest placeholder; the back-edge from EndFor still
        // targets this block.
        self.blocks[header].term = Some(IrTerminator::Unreachable);
        self.current = body;
        self.loop_stack.push(LoopFrame { header, exit });
    }

    fn end_for(&mut self) {
        let frame = self
            .loop_stack
            .pop()
            .expect("backend_ir: EndFor marker without a matching BeginFor");
        let header_id = self.blocks[frame.header].id;
        self.close_if_open(IrTerminator::Jump { block: header_id });
        self.current = frame.exit;
    }

    // -- match ---------------------------------------------------------------

    fn begin_match(&mut self, value: &str) {
        let scrutinee = self.resolve(value);
        self.ensure_open();
        let dispatch = self.current;
        let fallback = self.new_block();
        // The dispatch terminator carries an empty arm list until
        // EndMatch assembles the final one; the scrutinee block is
        // terminated at all times (block discipline holds mid-build).
        self.blocks[dispatch].term = Some(IrTerminator::MatchDispatch {
            scrutinee,
            arms: Vec::new(),
            fallback: self.block_id(fallback),
        });
        self.match_stack.push(MatchFrame {
            dispatch,
            fallback,
            join: None,
            arms: Vec::new(),
            scrutinee,
        });
    }

    fn match_join(&mut self) -> u32 {
        let existing = self.match_stack.last().and_then(|frame| frame.join);
        match existing {
            Some(index) => self.blocks[index].id,
            None => {
                let index = self.new_block();
                let id = self.blocks[index].id;
                self.match_stack
                    .last_mut()
                    .expect("backend_ir: match join requested without an open match frame")
                    .join = Some(index);
                id
            }
        }
    }

    fn match_arm(&mut self, enum_name: &str, variant: &str, bindings: &[String]) {
        // Close the previous arm's tail into the join, then open this
        // arm's block.
        let join = self.match_join();
        self.close_if_open(IrTerminator::Jump { block: join });
        let arm = self.new_block();
        let arm_id = self.block_id(arm);
        let enum_sym = self.module.intern(enum_name);
        let variant_sym = self.module.intern(variant);
        let binding_syms = bindings
            .iter()
            .map(|binding| self.module.intern(binding))
            .collect::<Vec<_>>();
        let frame = self
            .match_stack
            .last_mut()
            .expect("backend_ir: MatchArm marker without a matching BeginMatch");
        frame.arms.push(IrMatchArm {
            enum_name: enum_sym,
            variant: variant_sym,
            bindings: binding_syms,
            block: arm_id,
        });
        self.current = arm;
    }

    fn end_match(&mut self) {
        let frame = self
            .match_stack
            .pop()
            .expect("backend_ir: EndMatch marker without a matching BeginMatch");
        let join_index = frame.join.unwrap_or_else(|| self.new_block());
        let join_id = self.blocks[join_index].id;
        self.close_if_open(IrTerminator::Jump { block: join_id });
        // The "no arm matched" path flows to the join as well.
        self.blocks[frame.fallback].term = Some(IrTerminator::Jump { block: join_id });
        // Assemble the real dispatch: same scrutinee and fallback,
        // with the arms collected from the MatchArm markers.
        self.blocks[frame.dispatch].term = Some(IrTerminator::MatchDispatch {
            scrutinee: frame.scrutinee,
            arms: frame.arms,
            fallback: self.block_id(frame.fallback),
        });
        self.current = join_index;
    }

    // -- select (v1 deferral) --------------------------------------------------

    fn begin_select(&mut self) {
        // SELECT V1 DEFERRAL: real select lowering lands with the
        // async state-machine work (Phase 2+). The region lowers to
        // Unreachable-terminated blocks; arm instructions around the
        // markers are preserved verbatim; no dispatch edges are
        // invented. See the module docs.
        self.close_if_open(IrTerminator::Unreachable);
    }

    fn select_arm(&mut self, _operation: Option<&str>, _binding: Option<&str>, _is_default: bool) {
        // SELECT V1 DEFERRAL: the arm's operation instructions and the
        // MIR-emitted binding StoreLocal around this marker lower as
        // ordinary instructions; this marker only marks the arm
        // boundary as an Unreachable-closed block so the future
        // lowering has the structure to replace.
        self.close_if_open(IrTerminator::Unreachable);
        self.current = self.new_block();
    }

    fn end_select(&mut self) {
        // SELECT V1 DEFERRAL: the continuation block after the select
        // is unreachable in v1 but keeps every following instruction
        // in a well-formed block.
        self.close_if_open(IrTerminator::Unreachable);
        self.current = self.new_block();
    }

    // -- break / continue / return ---------------------------------------------

    fn break_stmt(&mut self) {
        let exit = self.loop_stack.last().map(|frame| frame.exit);
        self.ensure_open();
        let term = match exit {
            Some(index) => IrTerminator::Jump {
                block: self.blocks[index].id,
            },
            // Outside a loop: the typed layer already rejects this;
            // Unreachable keeps the block disciplined instead of
            // silently dropping the statement.
            None => IrTerminator::Unreachable,
        };
        self.blocks[self.current].term = Some(term);
    }

    fn continue_stmt(&mut self) {
        let header = self.loop_stack.last().map(|frame| frame.header);
        self.ensure_open();
        let term = match header {
            Some(index) => IrTerminator::Jump {
                block: self.blocks[index].id,
            },
            None => IrTerminator::Unreachable,
        };
        self.blocks[self.current].term = Some(term);
    }

    fn return_stmt(&mut self, value: &str) {
        let id = self.resolve(value);
        self.ensure_open();
        self.blocks[self.current].term = Some(IrTerminator::Return { value: Some(id) });
    }

    /// Finalizes the function: the trailing block (if open) gets the
    /// implicit unit return, and every block must then be terminated.
    fn finish(mut self) -> Vec<IrBlock> {
        if self.blocks[self.current].term.is_none() {
            self.blocks[self.current].term = Some(IrTerminator::Return { value: None });
        }
        self.blocks
            .into_iter()
            .enumerate()
            .map(|(index, raw)| IrBlock {
                id: raw.id,
                insts: raw.insts,
                terminator: raw.term.unwrap_or_else(|| {
                    panic!("backend_ir: block {index} was left without a terminator")
                }),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Canonical binary encoding
// ---------------------------------------------------------------------------

/// Encodes a module. Canonical: identical IR produces identical bytes
/// (fixed field order, no padding, no map iteration in the encoding
/// path).
pub fn encode(ir: &IrModule) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(wire::MAGIC);
    out.extend_from_slice(&wire::VERSION.to_le_bytes());
    write_varint(&mut out, ir.strings.len() as u64);
    for text in &ir.strings {
        write_varint(&mut out, text.len() as u64);
        out.extend_from_slice(text.as_bytes());
    }
    write_varint(&mut out, ir.functions.len() as u64);
    for function in &ir.functions {
        write_symbol(&mut out, function.name);
        write_varint(&mut out, function.params.len() as u64);
        for param in &function.params {
            write_value(&mut out, *param);
        }
        encode_type_tag(&mut out, &function.return_type);
        write_varint(&mut out, function.blocks.len() as u64);
        for block in &function.blocks {
            write_varint(&mut out, block.id as u64);
            write_varint(&mut out, block.insts.len() as u64);
            for inst in &block.insts {
                encode_inst(&mut out, inst);
            }
            encode_terminator(&mut out, &block.terminator);
        }
    }
    out
}

fn encode_type_tag(out: &mut Vec<u8>, tag: &IrTypeTag) {
    match tag {
        IrTypeTag::Int => out.push(wire::TYPE_INT),
        IrTypeTag::String => out.push(wire::TYPE_STRING),
        IrTypeTag::Bool => out.push(wire::TYPE_BOOL),
        IrTypeTag::Unit => out.push(wire::TYPE_UNIT),
        IrTypeTag::Json => out.push(wire::TYPE_JSON),
        IrTypeTag::List => out.push(wire::TYPE_LIST),
        IrTypeTag::Dict => out.push(wire::TYPE_DICT),
        IrTypeTag::Channel => out.push(wire::TYPE_CHANNEL),
        IrTypeTag::Task => out.push(wire::TYPE_TASK),
        IrTypeTag::Option => out.push(wire::TYPE_OPTION),
        IrTypeTag::Result => out.push(wire::TYPE_RESULT),
        IrTypeTag::Struct(sym) => {
            out.push(wire::TYPE_STRUCT);
            write_symbol(out, *sym);
        }
        IrTypeTag::Enum(sym) => {
            out.push(wire::TYPE_ENUM);
            write_symbol(out, *sym);
        }
        IrTypeTag::Opaque(sym) => {
            out.push(wire::TYPE_OPAQUE);
            write_symbol(out, *sym);
        }
        IrTypeTag::Unknown => out.push(wire::TYPE_UNKNOWN),
    }
}

fn encode_inst(out: &mut Vec<u8>, inst: &IrInst) {
    match &inst.kind {
        IrInstKind::ConstInt(value) => {
            out.push(wire::CONST_INT);
            write_result_slot(out, inst.result);
            write_varint(out, zigzag_encode(*value));
        }
        IrInstKind::ConstString(sym) => {
            out.push(wire::CONST_STRING);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
        }
        IrInstKind::ConstBool(flag) => {
            out.push(wire::CONST_BOOL);
            write_result_slot(out, inst.result);
            write_bool(out, *flag);
        }
        IrInstKind::ConstEnumVariant {
            enum_name,
            variant,
            args,
        } => {
            out.push(wire::CONST_ENUM_VARIANT);
            write_result_slot(out, inst.result);
            write_symbol(out, *enum_name);
            write_symbol(out, *variant);
            write_values(out, args);
        }
        IrInstKind::BuildList(items) => {
            out.push(wire::BUILD_LIST);
            write_result_slot(out, inst.result);
            write_values(out, items);
        }
        IrInstKind::BuildDict(entries) => {
            out.push(wire::BUILD_DICT);
            write_result_slot(out, inst.result);
            write_varint(out, entries.len() as u64);
            for (key, value) in entries {
                write_value(out, *key);
                write_value(out, *value);
            }
        }
        IrInstKind::BuildStruct { sym, fields } => {
            out.push(wire::BUILD_STRUCT);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
            write_values(out, fields);
        }
        IrInstKind::LoadLocal { sym } => {
            out.push(wire::LOAD_LOCAL);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
        }
        IrInstKind::LoadField { value, sym } => {
            out.push(wire::LOAD_FIELD);
            write_result_slot(out, inst.result);
            write_value(out, *value);
            write_symbol(out, *sym);
        }
        IrInstKind::Index { target, index } => {
            out.push(wire::INDEX);
            write_result_slot(out, inst.result);
            write_value(out, *target);
            write_value(out, *index);
        }
        IrInstKind::Unary { op, value } => {
            out.push(wire::UNARY);
            write_result_slot(out, inst.result);
            out.push(*op as u8);
            write_value(out, *value);
        }
        IrInstKind::Binary { op, lhs, rhs } => {
            out.push(wire::BINARY);
            write_result_slot(out, inst.result);
            out.push(*op as u8);
            write_value(out, *lhs);
            write_value(out, *rhs);
        }
        IrInstKind::Call { sym, args } => {
            out.push(wire::CALL);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
            write_values(out, args);
        }
        IrInstKind::Spawn { sym, args } => {
            out.push(wire::SPAWN);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
            write_values(out, args);
        }
        IrInstKind::Await { task } => {
            out.push(wire::AWAIT);
            write_result_slot(out, inst.result);
            write_value(out, *task);
        }
        IrInstKind::Propagate { value } => {
            out.push(wire::PROPAGATE);
            write_result_slot(out, inst.result);
            write_value(out, *value);
        }
        IrInstKind::StoreLocal {
            sym,
            value,
            mutable,
            declare,
        } => {
            out.push(wire::STORE_LOCAL);
            write_result_slot(out, inst.result);
            write_symbol(out, *sym);
            write_value(out, *value);
            write_bool(out, *mutable);
            write_bool(out, *declare);
        }
        IrInstKind::Eval { value } => {
            out.push(wire::EVAL);
            write_result_slot(out, inst.result);
            write_value(out, *value);
        }
    }
}

fn encode_terminator(out: &mut Vec<u8>, term: &IrTerminator) {
    match term {
        IrTerminator::Jump { block } => {
            out.push(wire::JUMP);
            write_varint(out, *block as u64);
        }
        IrTerminator::Branch {
            cond,
            then_block,
            else_block,
        } => {
            out.push(wire::BRANCH);
            write_value(out, *cond);
            write_varint(out, *then_block as u64);
            write_varint(out, *else_block as u64);
        }
        IrTerminator::MatchDispatch {
            scrutinee,
            arms,
            fallback,
        } => {
            out.push(wire::MATCH_DISPATCH);
            write_value(out, *scrutinee);
            write_varint(out, arms.len() as u64);
            for arm in arms {
                write_symbol(out, arm.enum_name);
                write_symbol(out, arm.variant);
                write_varint(out, arm.bindings.len() as u64);
                for binding in &arm.bindings {
                    write_symbol(out, *binding);
                }
                write_varint(out, arm.block as u64);
            }
            write_varint(out, *fallback as u64);
        }
        IrTerminator::Return { value } => {
            out.push(wire::RETURN);
            write_result_slot(out, *value);
        }
        IrTerminator::Unreachable => out.push(wire::UNREACHABLE),
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn zigzag_encode(value: i64) -> u64 {
    ((value << 1) as u64) ^ ((value >> 63) as u64)
}

fn zigzag_decode(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

fn write_symbol(out: &mut Vec<u8>, symbol: IrSymbol) {
    write_varint(out, symbol as u64);
}

fn write_value(out: &mut Vec<u8>, id: IrValueId) {
    write_varint(out, id as u64);
}

fn write_bool(out: &mut Vec<u8>, value: bool) {
    out.push(if value { 1 } else { 0 });
}

fn write_result_slot(out: &mut Vec<u8>, result: Option<IrValueId>) {
    match result {
        None => out.push(0),
        Some(id) => {
            out.push(1);
            write_value(out, id);
        }
    }
}

fn write_values(out: &mut Vec<u8>, ids: &[IrValueId]) {
    write_varint(out, ids.len() as u64);
    for id in ids {
        write_value(out, *id);
    }
}

// ---------------------------------------------------------------------------
// Decoding (validating)
// ---------------------------------------------------------------------------

/// Decodes and validates a module. Malformed input of every kind
/// (truncation, bad magic/version/tags, out-of-range ids, non-UTF-8,
/// use-before-def, trailing bytes) is an `Err`; this function never
/// panics on any byte sequence.
pub fn decode(bytes: &[u8]) -> Result<IrModule, BackendIrError> {
    let mut reader = Reader::new(bytes);
    if reader.read_bytes(5)? != wire::MAGIC.as_slice() {
        return Err(BackendIrError::BadMagic);
    }
    let version = u16::from_le_bytes([reader.read_u8()?, reader.read_u8()?]);
    if version != wire::VERSION {
        return Err(BackendIrError::UnsupportedVersion(version));
    }
    let string_count = reader.read_count("string table count")?;
    let mut strings = Vec::new();
    for _ in 0..string_count {
        let len = reader.read_count("string entry length")?;
        let raw = reader.read_bytes(len)?;
        let text = std::str::from_utf8(raw)
            .map_err(|_| BackendIrError::InvalidUtf8)?
            .to_string();
        strings.push(text);
    }
    let function_count = reader.read_count("function count")?;
    let mut functions = Vec::new();
    for _ in 0..function_count {
        functions.push(decode_function(&mut reader, &strings)?);
    }
    if reader.remaining() != 0 {
        return Err(BackendIrError::TrailingBytes);
    }
    let module = IrModule { functions, strings };
    validate(&module)?;
    Ok(module)
}

fn decode_function(reader: &mut Reader, strings: &[String]) -> Result<IrFunction, BackendIrError> {
    let name = reader.read_symbol(strings.len())?;
    let param_count = reader.read_count("param count")?;
    let mut params = Vec::new();
    for _ in 0..param_count {
        params.push(reader.read_u32("param value id")?);
    }
    let return_type = decode_type_tag(reader, strings.len())?;
    let block_count = reader.read_count("block count")?;
    let mut blocks = Vec::new();
    for index in 0..block_count {
        let id = reader.read_u32("block id")?;
        if id != index as u32 {
            return Err(BackendIrError::BlockIdMismatch { index, id });
        }
        let inst_count = reader.read_count("instruction count")?;
        let mut insts = Vec::new();
        for _ in 0..inst_count {
            insts.push(decode_inst(reader, strings.len())?);
        }
        let terminator = decode_terminator(reader, strings.len(), block_count)?;
        blocks.push(IrBlock {
            id,
            insts,
            terminator,
        });
    }
    Ok(IrFunction {
        name,
        params,
        return_type,
        blocks,
    })
}

fn decode_type_tag(reader: &mut Reader, table_len: usize) -> Result<IrTypeTag, BackendIrError> {
    let tag = reader.read_u8()?;
    Ok(match tag {
        wire::TYPE_INT => IrTypeTag::Int,
        wire::TYPE_STRING => IrTypeTag::String,
        wire::TYPE_BOOL => IrTypeTag::Bool,
        wire::TYPE_UNIT => IrTypeTag::Unit,
        wire::TYPE_JSON => IrTypeTag::Json,
        wire::TYPE_LIST => IrTypeTag::List,
        wire::TYPE_DICT => IrTypeTag::Dict,
        wire::TYPE_CHANNEL => IrTypeTag::Channel,
        wire::TYPE_TASK => IrTypeTag::Task,
        wire::TYPE_OPTION => IrTypeTag::Option,
        wire::TYPE_RESULT => IrTypeTag::Result,
        wire::TYPE_STRUCT => IrTypeTag::Struct(reader.read_symbol(table_len)?),
        wire::TYPE_ENUM => IrTypeTag::Enum(reader.read_symbol(table_len)?),
        wire::TYPE_OPAQUE => IrTypeTag::Opaque(reader.read_symbol(table_len)?),
        wire::TYPE_UNKNOWN => IrTypeTag::Unknown,
        _ => {
            return Err(BackendIrError::InvalidTag {
                context: "type",
                tag,
            });
        }
    })
}

fn decode_inst(reader: &mut Reader, table_len: usize) -> Result<IrInst, BackendIrError> {
    let tag = reader.read_u8()?;
    if !matches!(tag, wire::CONST_INT..=wire::EVAL) {
        return Err(BackendIrError::InvalidTag {
            context: "instruction",
            tag,
        });
    }
    let result = read_result_slot(reader)?;
    let kind = match tag {
        wire::CONST_INT => IrInstKind::ConstInt(zigzag_decode(reader.read_varint()?)),
        wire::CONST_STRING => IrInstKind::ConstString(reader.read_symbol(table_len)?),
        wire::CONST_BOOL => IrInstKind::ConstBool(reader.read_bool("const bool")?),
        wire::CONST_ENUM_VARIANT => {
            let enum_name = reader.read_symbol(table_len)?;
            let variant = reader.read_symbol(table_len)?;
            let args = read_values(reader)?;
            IrInstKind::ConstEnumVariant {
                enum_name,
                variant,
                args,
            }
        }
        wire::BUILD_LIST => IrInstKind::BuildList(read_values(reader)?),
        wire::BUILD_DICT => {
            let count = reader.read_count("dict entry count")?;
            let mut entries = Vec::new();
            for _ in 0..count {
                let key = reader.read_u32("dict key")?;
                let value = reader.read_u32("dict value")?;
                entries.push((key, value));
            }
            IrInstKind::BuildDict(entries)
        }
        wire::BUILD_STRUCT => {
            let sym = reader.read_symbol(table_len)?;
            IrInstKind::BuildStruct {
                sym,
                fields: read_values(reader)?,
            }
        }
        wire::LOAD_LOCAL => IrInstKind::LoadLocal {
            sym: reader.read_symbol(table_len)?,
        },
        wire::LOAD_FIELD => {
            let value = reader.read_u32("load field target")?;
            let sym = reader.read_symbol(table_len)?;
            IrInstKind::LoadField { value, sym }
        }
        wire::INDEX => {
            let target = reader.read_u32("index target")?;
            let index = reader.read_u32("index index")?;
            IrInstKind::Index { target, index }
        }
        wire::UNARY => {
            let op = read_unary_op(reader)?;
            let value = reader.read_u32("unary operand")?;
            IrInstKind::Unary { op, value }
        }
        wire::BINARY => {
            let op = read_binary_op(reader)?;
            let lhs = reader.read_u32("binary lhs")?;
            let rhs = reader.read_u32("binary rhs")?;
            IrInstKind::Binary { op, lhs, rhs }
        }
        wire::CALL => {
            let sym = reader.read_symbol(table_len)?;
            let args = read_values(reader)?;
            IrInstKind::Call { sym, args }
        }
        wire::SPAWN => {
            let sym = reader.read_symbol(table_len)?;
            let args = read_values(reader)?;
            IrInstKind::Spawn { sym, args }
        }
        wire::AWAIT => IrInstKind::Await {
            task: reader.read_u32("await task")?,
        },
        wire::PROPAGATE => IrInstKind::Propagate {
            value: reader.read_u32("propagate value")?,
        },
        wire::STORE_LOCAL => {
            let sym = reader.read_symbol(table_len)?;
            let value = reader.read_u32("store value")?;
            let mutable = reader.read_bool("store mutable flag")?;
            let declare = reader.read_bool("store declare flag")?;
            IrInstKind::StoreLocal {
                sym,
                value,
                mutable,
                declare,
            }
        }
        wire::EVAL => IrInstKind::Eval {
            value: reader.read_u32("eval value")?,
        },
        _ => {
            return Err(BackendIrError::InvalidTag {
                context: "instruction",
                tag,
            });
        }
    };
    Ok(IrInst { result, kind })
}

fn decode_terminator(
    reader: &mut Reader,
    table_len: usize,
    block_count: usize,
) -> Result<IrTerminator, BackendIrError> {
    let tag = reader.read_u8()?;
    Ok(match tag {
        wire::JUMP => {
            let block = reader.read_u32("jump target")?;
            check_target(block, block_count)?;
            IrTerminator::Jump { block }
        }
        wire::BRANCH => {
            let cond = reader.read_u32("branch condition")?;
            let then_block = reader.read_u32("branch then target")?;
            check_target(then_block, block_count)?;
            let else_block = reader.read_u32("branch else target")?;
            check_target(else_block, block_count)?;
            IrTerminator::Branch {
                cond,
                then_block,
                else_block,
            }
        }
        wire::MATCH_DISPATCH => {
            let scrutinee = reader.read_u32("match scrutinee")?;
            let arm_count = reader.read_count("match arm count")?;
            let mut arms = Vec::new();
            for _ in 0..arm_count {
                let enum_name = reader.read_symbol(table_len)?;
                let variant = reader.read_symbol(table_len)?;
                let binding_count = reader.read_count("match binding count")?;
                let mut bindings = Vec::new();
                for _ in 0..binding_count {
                    bindings.push(reader.read_symbol(table_len)?);
                }
                let block = reader.read_u32("match arm target")?;
                check_target(block, block_count)?;
                arms.push(IrMatchArm {
                    enum_name,
                    variant,
                    bindings,
                    block,
                });
            }
            let fallback = reader.read_u32("match fallback target")?;
            check_target(fallback, block_count)?;
            IrTerminator::MatchDispatch {
                scrutinee,
                arms,
                fallback,
            }
        }
        wire::RETURN => IrTerminator::Return {
            value: read_result_slot(reader)?,
        },
        wire::UNREACHABLE => IrTerminator::Unreachable,
        _ => {
            return Err(BackendIrError::InvalidTag {
                context: "terminator",
                tag,
            });
        }
    })
}

fn read_unary_op(reader: &mut Reader) -> Result<IrUnaryOp, BackendIrError> {
    match reader.read_u8()? {
        wire::OP_NOT => Ok(IrUnaryOp::Not),
        wire::OP_NEG => Ok(IrUnaryOp::Neg),
        tag => Err(BackendIrError::InvalidTag {
            context: "unary op",
            tag,
        }),
    }
}

fn read_binary_op(reader: &mut Reader) -> Result<IrBinaryOp, BackendIrError> {
    match reader.read_u8()? {
        wire::OP_ADD => Ok(IrBinaryOp::Add),
        wire::OP_SUB => Ok(IrBinaryOp::Sub),
        wire::OP_MUL => Ok(IrBinaryOp::Mul),
        wire::OP_DIV => Ok(IrBinaryOp::Div),
        wire::OP_MOD => Ok(IrBinaryOp::Mod),
        wire::OP_AND => Ok(IrBinaryOp::And),
        wire::OP_OR => Ok(IrBinaryOp::Or),
        wire::OP_EQ => Ok(IrBinaryOp::Eq),
        wire::OP_NE => Ok(IrBinaryOp::Ne),
        wire::OP_LT => Ok(IrBinaryOp::Lt),
        wire::OP_LE => Ok(IrBinaryOp::Le),
        wire::OP_GT => Ok(IrBinaryOp::Gt),
        wire::OP_GE => Ok(IrBinaryOp::Ge),
        tag => Err(BackendIrError::InvalidTag {
            context: "binary op",
            tag,
        }),
    }
}

fn read_result_slot(reader: &mut Reader) -> Result<Option<IrValueId>, BackendIrError> {
    match reader.read_u8()? {
        0 => Ok(None),
        1 => Ok(Some(reader.read_u32("result value id")?)),
        byte => Err(BackendIrError::InvalidByte {
            context: "result slot",
            byte,
        }),
    }
}

fn read_values(reader: &mut Reader) -> Result<Vec<IrValueId>, BackendIrError> {
    let count = reader.read_count("value list count")?;
    let mut ids = Vec::new();
    for _ in 0..count {
        ids.push(reader.read_u32("value id")?);
    }
    Ok(ids)
}

fn check_target(target: u32, block_count: usize) -> Result<(), BackendIrError> {
    if target as usize >= block_count {
        return Err(BackendIrError::BlockTargetOutOfRange {
            target,
            block_count,
        });
    }
    Ok(())
}

/// Byte cursor with validating reads. No method ever panics; every
/// out-of-bounds or malformed read is an `Err`.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn read_u8(&mut self) -> Result<u8, BackendIrError> {
        let byte = *self.bytes.get(self.pos).ok_or(BackendIrError::Truncated)?;
        self.pos += 1;
        Ok(byte)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], BackendIrError> {
        if self.remaining() < len {
            return Err(BackendIrError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// LEB128. At most 10 bytes; bit loss past `u64` and
    /// continuation past the 10th byte are overflows, not values.
    fn read_varint(&mut self) -> Result<u64, BackendIrError> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            if shift >= 64 {
                return Err(BackendIrError::VarintOverflow);
            }
            let byte = self.read_u8()?;
            let payload = (byte & 0x7F) as u64;
            if shift == 63 && payload > 1 {
                return Err(BackendIrError::VarintOverflow);
            }
            result |= payload << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
        }
    }

    fn read_u32(&mut self, context: &'static str) -> Result<u32, BackendIrError> {
        let value = self.read_varint()?;
        u32::try_from(value).map_err(|_| BackendIrError::IntegerTooLarge { context, value })
    }

    fn read_bool(&mut self, context: &'static str) -> Result<bool, BackendIrError> {
        match self.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            byte => Err(BackendIrError::InvalidByte { context, byte }),
        }
    }

    /// A list count: every list entry costs at least one byte, so a
    /// count larger than the remaining input is malformed (and never
    /// a plausible allocation).
    fn read_count(&mut self, context: &'static str) -> Result<usize, BackendIrError> {
        let count = self.read_varint()?;
        let count = usize::try_from(count)
            .map_err(|_| BackendIrError::CountExceedsRemaining { context })?;
        if count > self.remaining() {
            return Err(BackendIrError::CountExceedsRemaining { context });
        }
        Ok(count)
    }

    fn read_symbol(&mut self, table_len: usize) -> Result<IrSymbol, BackendIrError> {
        let symbol = self.read_u32("symbol index")?;
        if symbol as usize >= table_len {
            return Err(BackendIrError::SymbolOutOfRange { symbol, table_len });
        }
        Ok(symbol)
    }
}

// ---------------------------------------------------------------------------
// Structural validation
// ---------------------------------------------------------------------------

/// Checks the IR invariants on an already-constructed module (the
/// same checks `decode` applies): block ids equal indexes, terminator
/// targets exist, symbol indexes are in range, every value id is
/// defined exactly once module-wide, and every operand references an
/// id defined earlier in the same function. Also enforces that
/// `StoreLocal`/`Eval` never bind a result. A resultless
/// value-instruction (e.g. a `ConstInt` with `result: None`) is
/// tolerated by v1 — dead but structurally sound. Returns `Err` on
/// the first violation; never panics.
pub fn validate(ir: &IrModule) -> Result<(), BackendIrError> {
    let mut defined_module_wide: HashSet<IrValueId> = HashSet::new();
    for function in &ir.functions {
        if function.name as usize >= ir.strings.len() {
            return Err(BackendIrError::SymbolOutOfRange {
                symbol: function.name,
                table_len: ir.strings.len(),
            });
        }
        validate_type_tag(&function.return_type, ir.strings.len())?;
        let mut defined: HashSet<IrValueId> = HashSet::new();
        for &param in &function.params {
            if !defined_module_wide.insert(param) {
                return Err(BackendIrError::ValueRedefined { id: param });
            }
            defined.insert(param);
        }
        for (index, block) in function.blocks.iter().enumerate() {
            if block.id != index as u32 {
                return Err(BackendIrError::BlockIdMismatch {
                    index,
                    id: block.id,
                });
            }
            for inst in &block.insts {
                for symbol in inst_symbols(&inst.kind) {
                    if symbol as usize >= ir.strings.len() {
                        return Err(BackendIrError::SymbolOutOfRange {
                            symbol,
                            table_len: ir.strings.len(),
                        });
                    }
                }
                for operand in inst_operands(&inst.kind) {
                    if !defined.contains(&operand) {
                        return Err(BackendIrError::ValueNotDefined {
                            id: operand,
                            context: "instruction operand",
                        });
                    }
                }
                if let Some(id) = inst.result {
                    if matches!(
                        inst.kind,
                        IrInstKind::StoreLocal { .. } | IrInstKind::Eval { .. }
                    ) {
                        return Err(BackendIrError::SideEffectWithResult);
                    }
                    if !defined.insert(id) {
                        return Err(BackendIrError::ValueRedefined { id });
                    }
                    if !defined_module_wide.insert(id) {
                        return Err(BackendIrError::ValueRedefined { id });
                    }
                }
            }
            for symbol in terminator_symbols(&block.terminator) {
                if symbol as usize >= ir.strings.len() {
                    return Err(BackendIrError::SymbolOutOfRange {
                        symbol,
                        table_len: ir.strings.len(),
                    });
                }
            }
            for operand in terminator_operands(&block.terminator) {
                if !defined.contains(&operand) {
                    return Err(BackendIrError::ValueNotDefined {
                        id: operand,
                        context: "terminator operand",
                    });
                }
            }
            for target in terminator_targets(&block.terminator) {
                if target as usize >= function.blocks.len() {
                    return Err(BackendIrError::BlockTargetOutOfRange {
                        target,
                        block_count: function.blocks.len(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_type_tag(tag: &IrTypeTag, table_len: usize) -> Result<(), BackendIrError> {
    let symbol = match tag {
        IrTypeTag::Struct(symbol) | IrTypeTag::Enum(symbol) | IrTypeTag::Opaque(symbol) => {
            Some(*symbol)
        }
        _ => None,
    };
    if let Some(symbol) = symbol {
        if symbol as usize >= table_len {
            return Err(BackendIrError::SymbolOutOfRange { symbol, table_len });
        }
    }
    Ok(())
}

fn inst_operands(kind: &IrInstKind) -> Vec<IrValueId> {
    match kind {
        IrInstKind::ConstInt(_)
        | IrInstKind::ConstString(_)
        | IrInstKind::ConstBool(_)
        | IrInstKind::LoadLocal { .. } => Vec::new(),
        IrInstKind::ConstEnumVariant { args, .. } => args.clone(),
        IrInstKind::BuildList(items) => items.clone(),
        IrInstKind::BuildDict(entries) => entries.iter().flat_map(|(k, v)| [*k, *v]).collect(),
        IrInstKind::BuildStruct { fields, .. } => fields.clone(),
        IrInstKind::LoadField { value, .. } => vec![*value],
        IrInstKind::Index { target, index } => vec![*target, *index],
        IrInstKind::Unary { value, .. } => vec![*value],
        IrInstKind::Binary { lhs, rhs, .. } => vec![*lhs, *rhs],
        IrInstKind::Call { args, .. } | IrInstKind::Spawn { args, .. } => args.clone(),
        IrInstKind::Await { task } => vec![*task],
        IrInstKind::Propagate { value } | IrInstKind::Eval { value } => vec![*value],
        IrInstKind::StoreLocal { value, .. } => vec![*value],
    }
}

fn inst_symbols(kind: &IrInstKind) -> Vec<IrSymbol> {
    match kind {
        IrInstKind::ConstInt(_)
        | IrInstKind::ConstBool(_)
        | IrInstKind::BuildList(_)
        | IrInstKind::BuildDict(_)
        | IrInstKind::Index { .. }
        | IrInstKind::Unary { .. }
        | IrInstKind::Binary { .. }
        | IrInstKind::Await { .. }
        | IrInstKind::Propagate { .. }
        | IrInstKind::Eval { .. } => Vec::new(),
        IrInstKind::ConstString(sym) => vec![*sym],
        IrInstKind::ConstEnumVariant {
            enum_name, variant, ..
        } => vec![*enum_name, *variant],
        IrInstKind::BuildStruct { sym, .. } => vec![*sym],
        IrInstKind::LoadLocal { sym } => vec![*sym],
        IrInstKind::LoadField { sym, .. } => vec![*sym],
        IrInstKind::Call { sym, .. } | IrInstKind::Spawn { sym, .. } => vec![*sym],
        IrInstKind::StoreLocal { sym, .. } => vec![*sym],
    }
}

fn terminator_operands(term: &IrTerminator) -> Vec<IrValueId> {
    match term {
        IrTerminator::Jump { .. } | IrTerminator::Unreachable => Vec::new(),
        IrTerminator::Branch { cond, .. } => vec![*cond],
        IrTerminator::MatchDispatch { scrutinee, .. } => vec![*scrutinee],
        IrTerminator::Return { value } => match value {
            Some(id) => vec![*id],
            None => Vec::new(),
        },
    }
}

fn terminator_targets(term: &IrTerminator) -> Vec<u32> {
    match term {
        IrTerminator::Jump { block } => vec![*block],
        IrTerminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        IrTerminator::MatchDispatch { arms, fallback, .. } => {
            let mut targets: Vec<u32> = arms.iter().map(|arm| arm.block).collect();
            targets.push(*fallback);
            targets
        }
        IrTerminator::Return { .. } | IrTerminator::Unreachable => Vec::new(),
    }
}

fn terminator_symbols(term: &IrTerminator) -> Vec<IrSymbol> {
    match term {
        IrTerminator::MatchDispatch { arms, .. } => arms
            .iter()
            .flat_map(|arm| {
                let mut symbols = vec![arm.enum_name, arm.variant];
                symbols.extend(arm.bindings.iter().copied());
                symbols
            })
            .collect(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        BackendIrError, IrInstKind, IrTerminator, decode, encode, lower, validate, value_name,
        wire, write_varint,
    };
    use crate::hir::lower as lower_hir;
    use crate::mir::lower as lower_mir;
    use crate::module_graph::parse_single_source;
    use crate::source::SourceFile;
    use crate::ssa::{SsaFunction, SsaModule, lower as lower_ssa};
    use crate::typed_hir::{Type, lower as lower_typed};

    const ARITHMETIC_SOURCE: &str = "fn add(a, b):\n    return a + b\n\nfn main() -> int:\n    mut total = add(40, 2)\n    total = total + 1\n    return total\n";
    const BRANCHES_SOURCE: &str = "fn classify(x: int) -> string:\n    if x > 10:\n        return \"big\"\n    else:\n        return \"small\"\n\nfn main() -> string:\n    return classify(5)\n";
    const WHILE_SOURCE: &str = "fn main() -> bool:\n    mut x = 3\n    while x > 0:\n        x = x - 1\n    if x == 0:\n        return true\n    else:\n        return false\n";
    const FOR_SOURCE: &str = "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3, 4]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n";
    const MATCH_SOURCE: &str = "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn score(state: JobState) -> int:\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n\nfn main() -> int:\n    state: JobState = JobState.Running(42)\n    return score(state)\n";
    const SELECT_SOURCE: &str =
        "fn main() -> int:\n    select:\n        default:\n            return 1\n";
    const TASKS_SOURCE: &str = "fn lucky() -> int:\n    return 12\n\nfn main() -> int:\n    task = go lucky()\n    return await task\n";
    const PROPAGATE_SOURCE: &str = "fn double(value: int) -> Result[int, RuntimeError]:\n    return Result.Ok(value * 2)\n\nfn main() -> Result[int, RuntimeError]:\n    answer = double(21)?\n    return Result.Ok(answer)\n";
    const STRUCTS_SOURCE: &str = "struct Point:\n    x: int\n    y: int\n\nfn main() -> int:\n    point: Point = Point(1, 2)\n    return point.x + point.y\n";
    const UNARY_SOURCE: &str =
        "fn main() -> int:\n    flag = true\n    if not flag:\n        return -1\n    return 1\n";
    const DICT_SOURCE: &str = "fn main() -> bool:\n    left = {\"a\": 1, \"b\": 2}\n    right = {\"a\": 1, \"b\": 2}\n    return left == right\n";
    const LIST_INDEX_SOURCE: &str =
        "fn main() -> int:\n    values = [7, 8]\n    return values[0]\n";

    /// Runs the real pipeline pieces: parse -> HIR -> typed HIR -> MIR
    /// -> SSA (the exact chain `compile_source` uses, minus the
    /// module-graph/provider layer these import-free fixtures do not
    /// need).
    fn ssa_from_source(path: &str, source: &str) -> SsaModule {
        let file = SourceFile::new(path, source);
        let ast = parse_single_source(&file).expect("parse should succeed");
        let hir = lower_hir(&ast);
        let (typed, _warnings) = lower_typed(&hir).expect("typing should succeed");
        let mir = lower_mir(&typed);
        lower_ssa(&mir)
    }

    fn fixtures() -> Vec<(&'static str, &'static str)> {
        vec![
            ("arithmetic.gof", ARITHMETIC_SOURCE),
            ("branches.gof", BRANCHES_SOURCE),
            ("while_loop.gof", WHILE_SOURCE),
            ("for_loop.gof", FOR_SOURCE),
            ("match.gof", MATCH_SOURCE),
            ("select.gof", SELECT_SOURCE),
            ("tasks.gof", TASKS_SOURCE),
            ("propagate.gof", PROPAGATE_SOURCE),
            ("structs.gof", STRUCTS_SOURCE),
            ("unary.gof", UNARY_SOURCE),
            ("dict.gof", DICT_SOURCE),
            ("list_index.gof", LIST_INDEX_SOURCE),
        ]
    }

    fn encoded_arithmetic() -> Vec<u8> {
        encode(&lower(&ssa_from_source(
            "arithmetic.gof",
            ARITHMETIC_SOURCE,
        )))
    }

    // -- round trip / stability / determinism ---------------------------------

    #[test]
    fn round_trip_from_typed_source() {
        for (name, source) in fixtures() {
            let ir = lower(&ssa_from_source(name, source));
            validate(&ir).expect("lowered IR must validate");
            let bytes = encode(&ir);
            let decoded = decode(&bytes).expect("decode should succeed");
            validate(&decoded).expect("decoded IR must validate");
            assert_eq!(
                decoded, ir,
                "lower -> encode -> decode must reproduce the IR for {name}"
            );
        }
    }

    #[test]
    fn byte_stability_round_trip() {
        for (name, source) in fixtures() {
            let ir = lower(&ssa_from_source(name, source));
            let once = encode(&ir);
            let decoded = decode(&once).expect("decode should succeed");
            let twice = encode(&decoded);
            assert_eq!(
                once, twice,
                "encode(decode(encode(ir))) must be byte-stable for {name}"
            );
        }
    }

    #[test]
    fn lowering_is_deterministic() {
        for (name, source) in fixtures() {
            let first = encode(&lower(&ssa_from_source(name, source)));
            let second = encode(&lower(&ssa_from_source(name, source)));
            assert_eq!(
                first, second,
                "two full pipeline runs must encode identically for {name}"
            );
            let ssa = ssa_from_source(name, source);
            assert_eq!(
                encode(&lower(&ssa)),
                encode(&lower(&ssa)),
                "two lowers of the same SSA module must encode identically for {name}"
            );
        }
    }

    // -- block discipline (structural assertions on lowered shapes) -----------

    #[test]
    fn every_block_is_terminated_and_targets_exist() {
        for (name, source) in fixtures() {
            let ir = lower(&ssa_from_source(name, source));
            for function in &ir.functions {
                assert!(
                    !function.blocks.is_empty(),
                    "every function has at least the entry block: {name}"
                );
                for block in &function.blocks {
                    // terminator presence is type-level; targets:
                    for target in super::terminator_targets(&block.terminator) {
                        assert!(
                            (target as usize) < function.blocks.len(),
                            "terminator target {target} must exist for {name}"
                        );
                    }
                }
            }
            validate(&ir).expect("validate must accept the lowered IR");
        }
    }

    #[test]
    fn while_condition_is_re_evaluated_in_the_header() {
        let ir = lower(&ssa_from_source("while_loop.gof", WHILE_SOURCE));
        let function = &ir.functions[0];
        // The while header is the first Branch block (created when
        // BeginWhile is seen, before the trailing if).
        let branch_index = function
            .blocks
            .iter()
            .position(|block| matches!(block.terminator, IrTerminator::Branch { .. }))
            .expect("a while loop must produce a Branch block");
        let branch_block = &function.blocks[branch_index];
        let cond = match &branch_block.terminator {
            IrTerminator::Branch { cond, .. } => *cond,
            other => panic!("expected a Branch terminator, got {other:?}"),
        };
        assert!(
            branch_block
                .insts
                .iter()
                .any(|inst| inst.result == Some(cond)),
            "the condition must be computed inside the loop header (moved out of the pre-header)"
        );
        let header = branch_block.id;
        let edges_into_header = function
            .blocks
            .iter()
            .filter(|block| {
                matches!(&block.terminator, IrTerminator::Jump { block } if *block == header)
            })
            .count();
        assert!(
            edges_into_header >= 2,
            "the header needs the pre-header edge and the loop back-edge, found {edges_into_header}"
        );
    }

    #[test]
    fn break_and_continue_target_the_loop_header_and_exit() {
        let source = "fn main() -> int:\n    mut total = 0\n    mut i = 0\n    while i < 4:\n        i = i + 1\n        if i == 2:\n            continue\n        total = total + i\n        if total > 3:\n            break\n    return total\n";
        let ir = lower(&ssa_from_source("loop_edges.gof", source));
        validate(&ir).expect("loop with break/continue must validate");
        let function = &ir.functions[0];
        // The while header is the block CONTAINING the Branch (its
        // then-target is the body, its else-target the exit).
        let header_index = function
            .blocks
            .iter()
            .position(|block| matches!(block.terminator, IrTerminator::Branch { .. }))
            .expect("expected the while header Branch");
        let exit = match &function.blocks[header_index].terminator {
            IrTerminator::Branch { else_block, .. } => *else_block,
            other => panic!("expected a Branch terminator, got {other:?}"),
        };
        let header = function.blocks[header_index].id;
        let jumps_to = |target: u32| {
            function
                .blocks
                .iter()
                .filter(|block| {
                    matches!(&block.terminator, IrTerminator::Jump { block } if *block == target)
                })
                .count()
        };
        // pre-header Jump + EndWhile back-edge + continue
        assert!(
            jumps_to(header) >= 3,
            "pre-header, EndWhile and continue must jump to the header"
        );
        // break
        assert!(jumps_to(exit) >= 1, "break must jump to the loop exit");
    }

    #[test]
    fn for_loop_keeps_back_edge_to_deferred_header() {
        let ir = lower(&ssa_from_source("for_loop.gof", FOR_SOURCE));
        let function = &ir.functions[0];
        let deferred_headers: Vec<u32> = function
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, IrTerminator::Unreachable))
            .map(|block| block.id)
            .collect();
        assert!(
            !deferred_headers.is_empty(),
            "the for-loop header must be the documented Unreachable deferral"
        );
        let back_edge = function.blocks.iter().any(|block| {
            matches!(&block.terminator, IrTerminator::Jump { block } if deferred_headers.contains(block))
        });
        assert!(
            back_edge,
            "EndFor (and continue) must jump back to the deferred loop header"
        );
        validate(&ir).expect("for loop with break/continue must validate");
    }

    #[test]
    fn select_lowers_to_documented_unreachable_deferral() {
        let ir = lower(&ssa_from_source("select.gof", SELECT_SOURCE));
        assert!(
            ir.functions[0]
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, IrTerminator::Unreachable)),
            "select must lower to the documented Unreachable deferral"
        );
        validate(&ir).expect("select deferral must validate");
    }

    #[test]
    fn match_lowers_to_dispatch_terminator_with_arms() {
        let ir = lower(&ssa_from_source("match.gof", MATCH_SOURCE));
        let score = &ir.functions[0];
        let dispatch = score
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, IrTerminator::MatchDispatch { .. }))
            .expect("match must produce a MatchDispatch terminator");
        let arms = match &dispatch.terminator {
            IrTerminator::MatchDispatch { arms, .. } => arms,
            other => panic!("expected MatchDispatch, got {other:?}"),
        };
        assert_eq!(arms.len(), 3, "the three source arms must be encoded");
        let variants: Vec<String> = arms
            .iter()
            .map(|arm| ir.strings[arm.variant as usize].clone())
            .collect();
        assert_eq!(variants, vec!["Ready", "Running", "Failed"]);
        assert!(
            arms.iter().any(|arm| !arm.bindings.is_empty()),
            "the payload arms must carry their binding names"
        );
    }

    #[test]
    fn unreachable_join_after_early_returns_stays_well_formed() {
        // Both arms of classify return; the join block is unreachable
        // but must still be a well-formed block (implicit unit
        // return).
        let ir = lower(&ssa_from_source("branches.gof", BRANCHES_SOURCE));
        let classify = &ir.functions[0];
        let returns = classify
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, IrTerminator::Return { .. }))
            .count();
        assert!(
            returns >= 3,
            "two arm returns plus the join's implicit return"
        );
        validate(&ir).expect("early returns must leave valid block structure");
    }

    // -- identities ----------------------------------------------------------

    #[test]
    fn value_name_formats_debug_identities() {
        assert_eq!(value_name(0), "v0");
        assert_eq!(value_name(7), "v7");
    }

    #[test]
    fn value_ids_are_module_global_in_first_definition_order() {
        let ir = lower(&ssa_from_source("arithmetic.gof", ARITHMETIC_SOURCE));
        assert_eq!(ir.functions.len(), 2);
        let add = &ir.functions[0];
        assert_eq!(add.params, vec![0, 1], "add's params are v0 and v1");
        let max_in_add = add
            .blocks
            .iter()
            .flat_map(|block| block.insts.iter().filter_map(|inst| inst.result))
            .max()
            .unwrap_or(1);
        let main = &ir.functions[1];
        let min_in_main = main
            .blocks
            .iter()
            .flat_map(|block| block.insts.iter().filter_map(|inst| inst.result))
            .min()
            .unwrap_or(u32::MAX);
        assert!(
            min_in_main > max_in_add,
            "main's ids must continue after add's (module-global counter)"
        );
    }

    #[test]
    fn empty_function_gets_implicit_unit_return() {
        let ssa = SsaModule {
            functions: vec![SsaFunction {
                name: "empty".to_string(),
                params: vec![],
                return_type: Type::Unit,
                values: vec![],
            }],
        };
        let ir = lower(&ssa);
        let function = &ir.functions[0];
        assert_eq!(function.blocks.len(), 1);
        assert!(matches!(
            function.blocks[0].terminator,
            IrTerminator::Return { value: None }
        ));
        let bytes = encode(&ir);
        let decoded = decode(&bytes).expect("decode should succeed");
        assert_eq!(decoded, ir);
    }

    // -- malformed inputs ------------------------------------------------------

    #[test]
    fn decode_rejects_truncated_bytes() {
        let bytes = encoded_arithmetic();
        for length in 0..bytes.len() {
            assert!(
                decode(&bytes[..length]).is_err(),
                "every strict prefix must fail to decode (length {length})"
            );
        }
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let mut bytes = encoded_arithmetic();
        bytes[0] = b'X';
        assert!(matches!(decode(&bytes), Err(BackendIrError::BadMagic)));
    }

    #[test]
    fn decode_rejects_bad_version() {
        for version in [0u16, 2, u16::MAX] {
            let mut bytes = encoded_arithmetic();
            bytes[5] = (version & 0xFF) as u8;
            bytes[6] = (version >> 8) as u8;
            assert!(
                matches!(&decode(&bytes), Err(BackendIrError::UnsupportedVersion(actual)) if *actual == version),
                "version {version} must be rejected"
            );
        }
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encoded_arithmetic();
        bytes.push(0);
        assert!(matches!(decode(&bytes), Err(BackendIrError::TrailingBytes)));
    }

    /// Hand-built encoding: one string ("f"), one function, one
    /// block. Byte layout (fixed offsets used by the tests below):
    /// 0..5 magic, 5..7 version, 7 string count, 8 string len, 9 "f",
    /// 10 function count, 11 name symbol, 12 param count, 13 return
    /// type tag, 14 block count, 15 block id, 16 instruction count,
    /// then the instruction bytes and the terminator bytes.
    fn one_block_module(insts: &[&[u8]], terminator: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(wire::MAGIC);
        bytes.extend_from_slice(&wire::VERSION.to_le_bytes());
        write_varint(&mut bytes, 1);
        write_varint(&mut bytes, 1);
        bytes.push(b'f');
        write_varint(&mut bytes, 1);
        write_varint(&mut bytes, 0);
        write_varint(&mut bytes, 0);
        bytes.push(wire::TYPE_INT);
        write_varint(&mut bytes, 1);
        write_varint(&mut bytes, 0);
        write_varint(&mut bytes, insts.len() as u64);
        for inst in insts {
            bytes.extend_from_slice(inst);
        }
        bytes.extend_from_slice(terminator);
        bytes
    }

    #[test]
    fn decode_rejects_unknown_instruction_tag() {
        let bytes = one_block_module(&[&[0xFF, 0, 10]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidTag {
                context: "instruction",
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_unknown_terminator_tag() {
        let bytes = one_block_module(&[&[wire::CONST_INT, 1, 0, 10]], &[0xFF]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidTag {
                context: "terminator",
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_unknown_type_tag() {
        let mut bytes = one_block_module(&[&[wire::CONST_INT, 1, 0, 10]], &[wire::UNREACHABLE]);
        bytes[13] = 0xFF; // return-type tag slot
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidTag {
                context: "type",
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_unknown_unary_op_tag() {
        let bytes = one_block_module(&[&[wire::UNARY, 1, 0, 0xFF, 0]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidTag {
                context: "unary op",
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_unknown_binary_op_tag() {
        let bytes = one_block_module(&[&[wire::BINARY, 1, 0, 0xFF, 0, 0]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidTag {
                context: "binary op",
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_out_of_range_symbol() {
        // ConstString binding symbol 5 while the table has one entry.
        let bytes = one_block_module(&[&[wire::CONST_STRING, 1, 0, 5]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::SymbolOutOfRange { symbol: 5, .. })
        ));
    }

    #[test]
    fn decode_rejects_undefined_value_operand() {
        // Binary add: result v0, operands v9/v9 — never defined.
        let bytes = one_block_module(&[&[wire::BINARY, 1, 0, 1, 9, 9]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::ValueNotDefined { id: 9, .. })
        ));
    }

    #[test]
    fn decode_rejects_duplicate_value_definition() {
        // Two ConstInt instructions both binding v0.
        let first = [wire::CONST_INT, 1, 0, 10];
        let second = [wire::CONST_INT, 1, 0, 10];
        let bytes = one_block_module(&[&first, &second], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::ValueRedefined { id: 0 })
        ));
    }

    #[test]
    fn decode_rejects_result_slot_on_side_effect_inst() {
        // v0 defined by a ConstInt, then a StoreLocal that illegally
        // binds a result.
        let const_int = [wire::CONST_INT, 1, 0, 10];
        let store = [wire::STORE_LOCAL, 1, 0, 0, 0, 0, 0];
        let bytes = one_block_module(&[&const_int, &store], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::SideEffectWithResult)
        ));
    }

    #[test]
    fn decode_rejects_block_target_out_of_range() {
        // Jump to block 3 in a one-block function.
        let bytes = one_block_module(&[&[wire::CONST_INT, 1, 0, 10]], &[wire::JUMP, 3]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::BlockTargetOutOfRange { target: 3, .. })
        ));
    }

    #[test]
    fn decode_rejects_invalid_result_slot_byte() {
        let bytes = one_block_module(&[&[wire::CONST_INT, 2, 0, 10]], &[wire::UNREACHABLE]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::InvalidByte {
                context: "result slot",
                byte: 2
            })
        ));
    }

    #[test]
    fn decode_rejects_invalid_utf8_string() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(wire::MAGIC);
        bytes.extend_from_slice(&wire::VERSION.to_le_bytes());
        write_varint(&mut bytes, 1);
        write_varint(&mut bytes, 1);
        bytes.push(0xFF);
        assert!(matches!(decode(&bytes), Err(BackendIrError::InvalidUtf8)));
    }

    #[test]
    fn decode_rejects_varint_overflow() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(wire::MAGIC);
        bytes.extend_from_slice(&wire::VERSION.to_le_bytes());
        bytes.extend_from_slice(&[0xFF; 10]);
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::VarintOverflow)
        ));
    }

    #[test]
    fn decode_rejects_count_exceeding_remaining_bytes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(wire::MAGIC);
        bytes.extend_from_slice(&wire::VERSION.to_le_bytes());
        write_varint(&mut bytes, 100); // 100 strings promised
        write_varint(&mut bytes, 1);
        bytes.push(b'f'); // only one entry present
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::CountExceedsRemaining { .. })
        ));
    }

    #[test]
    fn decode_rejects_block_id_mismatch() {
        let mut bytes = one_block_module(&[&[wire::CONST_INT, 1, 0, 10]], &[wire::UNREACHABLE]);
        bytes[15] = 1; // block id must equal its index (0)
        assert!(matches!(
            decode(&bytes),
            Err(BackendIrError::BlockIdMismatch { index: 0, id: 1 })
        ));
    }

    // -- validate negatives (hand-broken IR) ----------------------------------

    #[test]
    fn validate_rejects_broken_structures() {
        let base = lower(&ssa_from_source("while_loop.gof", WHILE_SOURCE));

        // branch target out of range
        let mut bad_target = base.clone();
        for block in &mut bad_target.functions[0].blocks {
            if let IrTerminator::Jump { block: target } = &mut block.terminator {
                *target = 99;
                break;
            }
        }
        assert!(matches!(
            validate(&bad_target),
            Err(BackendIrError::BlockTargetOutOfRange { .. })
        ));

        // operand referencing an undefined value
        let mut bad_operand = base.clone();
        'operand: for block in &mut bad_operand.functions[0].blocks {
            for inst in &mut block.insts {
                if let IrInstKind::Binary { lhs, .. } = &mut inst.kind {
                    *lhs = 900;
                    break 'operand;
                }
            }
        }
        assert!(matches!(
            validate(&bad_operand),
            Err(BackendIrError::ValueNotDefined { .. })
        ));

        // duplicate result id within one function
        let mut duplicate = base.clone();
        let mut first: Option<u32> = None;
        'duplicate: for block in &mut duplicate.functions[0].blocks {
            for inst in &mut block.insts {
                match (first, inst.result) {
                    (None, Some(result)) => first = Some(result),
                    (Some(previous), Some(_)) => {
                        inst.result = Some(previous);
                        break 'duplicate;
                    }
                    _ => {}
                }
            }
        }
        assert!(matches!(
            validate(&duplicate),
            Err(BackendIrError::ValueRedefined { .. })
        ));

        // symbol index beyond the string table
        let mut bad_symbol = base.clone();
        let mut rewired = false;
        'symbol: for block in &mut bad_symbol.functions[0].blocks {
            for inst in &mut block.insts {
                if let IrInstKind::LoadLocal { sym } = &mut inst.kind {
                    *sym = 900;
                    rewired = true;
                    break 'symbol;
                }
            }
        }
        assert!(rewired, "fixture must contain a LoadLocal to break");
        assert!(matches!(
            validate(&bad_symbol),
            Err(BackendIrError::SymbolOutOfRange { .. })
        ));
    }
}
