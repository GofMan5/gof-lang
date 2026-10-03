//! Backend codegen v1 (ADR 0004 dev profile): lowers
//! [`crate::backend_ir::IrModule`] to Cranelift and executes `main` as
//! REAL machine code through the JIT (`cranelift-jit`) — the first
//! artifact produced from the backend IR. Object emission + linking is
//! the next slice; the evaluator stays the conformance oracle and the
//! execution surface for everything this slice defers.
//!
//! # v1 scope (honest — no silent gaps)
//!
//! Machine execution covers int/bool programs end to end: int and bool
//! constants, unary/binary arithmetic and comparisons, gof→gof direct
//! calls (including recursion and multiple arguments), if/else, and
//! `while` loops in the IR's CONDITION-MOVE header form — the loop
//! header re-evaluates the condition per iteration in machine code,
//! which is the key proof that the IR loop shape lowers correctly.
//!
//! Value representation: every gof int and bool is one i64 machine
//! word (bools are 0/1). Locals are Cranelift `Variable`s (the
//! frontend's SSA construction carries loop values), keyed by the
//! IR's local symbols. All parameters compile as i64 — the v1 IR
//! carries no parameter types (the documented
//! [`crate::backend_ir::IrTypeTag`] fidelity limit); non-int argument
//! values surface as honest `UnsupportedConstruct` errors at their
//! constants. Functions may return `int` or `unit`; `main` must be
//! `fn main() -> int` with no parameters.
//!
//! Deferred with NAMED slices — every unsupported construct returns a
//! [`CodegenError`] naming it, never a silent skip:
//! - `match dispatch` (MatchDispatch terminator): needs the enum-tag
//!   layout + destructure slice; the evaluator remains the oracle for
//!   match programs
//! - `Unreachable` terminators: the for-loop header deferral, the
//!   select deferral, and invalid out-of-loop break/continue regions
//!   (all documented in `backend_ir`)
//! - string/list/dict/struct/enum VALUES: the value-representation
//!   slice (box or tagged layouts)
//! - `go`/`await`/`?`: the concurrency and result runtime-ABI slices
//! - builtin calls (`print`, `len`, ...): only module functions are
//!   callable in v1; a call to an undeclared symbol is an honest
//!   error naming it
//! - module-level `const` references lower as `LoadLocal` of an
//!   unbound name (the RFC 0054 note in `mir.rs`) — an honest error
//!
//! Semantics notes: `and`/`or` compile as strict (non-short-circuit)
//! `band`/`bor`, exactly what the IR's `Binary` algebra says (both
//! operands are already-evaluated SSA values). `sdiv`/`srem` trap the
//! process on divide-by-zero — machine code v1 has no gof-level trap
//! handler, so the evaluator remains the surface for diagnosed
//! arithmetic faults. INTEGER OVERFLOW diverges today (known v1
//! divergence, resolved by a future overflow-semantics spec decision):
//! the native path wraps (2's-complement machine behavior) while the
//! evaluator's Rust-checked arithmetic panics on the assertion — the
//! gof spec is currently silent on overflow. `Return` with no value in
//! an int function only
//! occurs in dead blocks (the unreachable join after early returns);
//! it compiles to an unreachable-code trap, so a broken invariant
//! fails loudly instead of silently returning 0.
//!
//! Layering (ADR 0002): this module imports `backend_ir` only;
//! `pipeline` is its only caller.

use crate::backend_ir::{
    IrBinaryOp, IrFunction, IrInst, IrInstKind, IrModule, IrSymbol, IrTerminator, IrTypeTag,
    IrUnaryOp, IrValueId,
};
use cranelift::codegen::Context;
use cranelift::codegen::ir::FuncRef;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::{HashMap, HashSet};

/// Construct name for the MatchDispatch v1 deferral. The IR carries
/// (enum, variant) symbols per arm but no enum-tag instruction, so the
/// dispatch test is not expressible; the evaluator remains the oracle
/// for match programs.
const MATCH_DISPATCH_DEFERRAL: &str = "match dispatch";

/// Construct name for `IrTerminator::Unreachable` — the documented v1
/// deferral regions: the for-loop header (no iteration primitive in
/// the instruction algebra), the select region (async state-machine
/// work), and `break`/`continue` outside a loop.
const UNREACHABLE_DEFERRAL: &str = "unreachable terminator (for-loop header / select / out-of-loop break-continue deferral region)";

/// Trap for the dead implicit `Return{None}` in int functions (the
/// unreachable join after early returns). A gof-owned user trap code:
/// executing a supposedly-dead terminator fails loudly and
/// distinguishably (cranelift 0.116 has no reserved
/// "unreachable code" trap), never a silent 0.
const DEAD_IMPLICIT_RETURN_TRAP: TrapCode = TrapCode::unwrap_user(1);

/// Codegen failure. Every variant names what it rejected — honest
/// diagnostics, never silent wrongness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    /// A function whose return type is outside the v1 machine-code
    /// surface (int/unit). Carries the function name and the IR type
    /// tag it rejected.
    UnsupportedType { function: String, ty: String },
    /// A construct the v1 lowering defers to a named later slice.
    UnsupportedConstruct { construct: String },
    /// The module has no callable `fn main() -> int` entry point.
    MissingMain,
    /// Cranelift JIT compilation or IR-shape failure (malformed input
    /// or a broken lowering invariant), reported with detail.
    JitError(String),
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedType { function, ty } => write!(
                f,
                "native codegen does not support return type `{ty}` on function `{function}` \
                 (v1 machine code covers int/unit returns)"
            ),
            Self::UnsupportedConstruct { construct } => {
                write!(f, "native codegen does not support construct: {construct}")
            }
            Self::MissingMain => {
                write!(
                    f,
                    "native execution requires a `fn main() -> int` entry point"
                )
            }
            Self::JitError(detail) => write!(f, "native codegen JIT failure: {detail}"),
        }
    }
}

impl std::error::Error for CodegenError {}

// ---------------------------------------------------------------------------
// Parameter binding (the v1 IR defers it to this slice)
// ---------------------------------------------------------------------------

/// Materializes the parameter→local binding the v1 IR defers to the
/// codegen slice: `IrFunction::params` carries the incoming-argument
/// value IDs (positional, never referenced by the body) while the body
/// reads parameters through `LoadLocal` by name. This function
/// prepends `StoreLocal` instructions to each function's entry block
/// (block 0) that bind argument `i`'s value id to local name
/// `param_names[i][i]`, so the returned module is a complete,
/// self-contained `IrModule` — the ordinary local data path carries
/// parameters, and nothing in [`execute`] needs out-of-band data.
///
/// `param_names[i]` must list function `i`'s parameter names in
/// declaration order (the pipeline threads them from the SSA layer,
/// which has them); a length mismatch is an honest [`CodegenError`].
pub fn bind_function_params(
    ir: &IrModule,
    param_names: &[Vec<String>],
) -> Result<IrModule, CodegenError> {
    if param_names.len() != ir.functions.len() {
        return Err(CodegenError::JitError(format!(
            "parameter binding mismatch: the module declares {} functions but {} parameter-name \
             lists were supplied",
            ir.functions.len(),
            param_names.len(),
        )));
    }

    let mut strings = ir.strings.clone();
    let mut interned: HashMap<&str, IrSymbol> = ir
        .strings
        .iter()
        .enumerate()
        .map(|(index, text)| (text.as_str(), index as IrSymbol))
        .collect();

    let functions = ir
        .functions
        .iter()
        .zip(param_names.iter())
        .map(|(function, names)| {
            if names.len() != function.params.len() {
                let function_name = strings
                    .get(function.name as usize)
                    .map(String::as_str)
                    .unwrap_or("?");
                return Err(CodegenError::JitError(format!(
                    "parameter binding mismatch for `{function_name}`: the IR declares {} \
                     parameters, {} names were supplied",
                    function.params.len(),
                    names.len(),
                )));
            }
            let mut bound = function.clone();
            if bound.blocks.is_empty() {
                return Err(CodegenError::JitError(format!(
                    "malformed IR: function `{}` has no blocks",
                    strings
                        .get(function.name as usize)
                        .map(String::as_str)
                        .unwrap_or("?")
                )));
            }
            for (position, (name, &param_id)) in
                names.iter().zip(function.params.iter()).enumerate()
            {
                let sym = match interned.get(name.as_str()) {
                    Some(&sym) => sym,
                    None => {
                        let sym = strings.len() as IrSymbol;
                        strings.push(name.clone());
                        interned.insert(name.as_str(), sym);
                        sym
                    }
                };
                // Insert in declaration order: param 0's store lands
                // first, ahead of every body instruction.
                bound.blocks[0].insts.insert(
                    position,
                    IrInst {
                        result: None,
                        kind: IrInstKind::StoreLocal {
                            sym,
                            value: param_id,
                            mutable: true,
                            declare: true,
                        },
                    },
                );
            }
            Ok(bound)
        })
        .collect::<Result<Vec<IrFunction>, CodegenError>>()?;

    Ok(IrModule { functions, strings })
}

// ---------------------------------------------------------------------------
// JIT execution
// ---------------------------------------------------------------------------

/// JIT-compiles the module through Cranelift and executes `main`,
/// returning its int return value as REAL machine code — no evaluator
/// involved. See the module docs for the v1 scope and the named
/// deferrals.
pub fn execute(ir: &IrModule) -> Result<i64, CodegenError> {
    let main_index = ir
        .functions
        .iter()
        .position(|function| symbol_text(ir, function.name) == Ok("main"))
        .ok_or(CodegenError::MissingMain)?;
    let main = &ir.functions[main_index];
    if !matches!(main.return_type, IrTypeTag::Int) {
        return Err(CodegenError::UnsupportedType {
            function: "main".to_string(),
            ty: format!("{:?}", main.return_type),
        });
    }
    if !main.params.is_empty() {
        return Err(CodegenError::UnsupportedConstruct {
            construct: "`main` with parameters (the native entry point takes no arguments)"
                .to_string(),
        });
    }

    let builder = JITBuilder::new(cranelift_module::default_libcall_names())
        .map_err(|error| CodegenError::JitError(error.to_string()))?;
    let mut module = JITModule::new(builder);

    // Pass 1: declare every function — direct calls need the ids and
    // signatures before any body is defined.
    let mut signatures: Vec<Signature> = Vec::with_capacity(ir.functions.len());
    let mut func_ids: Vec<FuncId> = Vec::with_capacity(ir.functions.len());
    let mut declared: HashMap<IrSymbol, FuncId> = HashMap::new();
    for function in &ir.functions {
        let name = symbol_text(ir, function.name)?;
        let mut signature = module.make_signature();
        for _ in &function.params {
            signature.params.push(AbiParam::new(types::I64));
        }
        match function.return_type {
            IrTypeTag::Int => signature.returns.push(AbiParam::new(types::I64)),
            IrTypeTag::Unit => {}
            ref other => {
                return Err(CodegenError::UnsupportedType {
                    function: name.to_string(),
                    ty: format!("{other:?}"),
                });
            }
        }
        let id = module
            .declare_function(name, Linkage::Export, &signature)
            .map_err(|error| CodegenError::JitError(error.to_string()))?;
        signatures.push(signature);
        func_ids.push(id);
        declared.insert(function.name, id);
    }

    // Pass 2: define each function body.
    for (index, function) in ir.functions.iter().enumerate() {
        let name = symbol_text(ir, function.name)?;
        let mut context = module.make_context();
        context.func.signature = signatures[index].clone();
        // Pre-declare call references for every module function: the
        // builder owns `context.func` exclusively while lowering, and
        // any function may call any other (including itself).
        let mut func_refs: HashMap<IrSymbol, FuncRef> = HashMap::new();
        for (sym, &callee_id) in &declared {
            let func_ref = module.declare_func_in_func(callee_id, &mut context.func);
            func_refs.insert(*sym, func_ref);
        }
        define_function(ir, function, name, &mut context, &func_refs)?;
        module
            .define_function(func_ids[index], &mut context)
            .map_err(|error| CodegenError::JitError(error.to_string()))?;
    }

    module
        .finalize_definitions()
        .map_err(|error| CodegenError::JitError(error.to_string()))?;

    let code = module.get_finalized_function(func_ids[main_index]);
    // SAFETY: `main` was declared and defined with the fixed
    // ()->i64 signature (enforced above); the returned code pointer
    // is executable and stays valid while `module` is alive.
    let main_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(code) };
    Ok(main_fn())
}

/// Resolves a symbol to its string-table text; an out-of-range index
/// is malformed IR, reported honestly.
fn symbol_text<'a>(ir: &'a IrModule, sym: IrSymbol) -> Result<&'a str, CodegenError> {
    ir.strings
        .get(sym as usize)
        .map(String::as_str)
        .ok_or_else(|| {
            CodegenError::JitError(format!(
                "malformed IR: symbol {sym} is out of range (string table holds {} entries)",
                ir.strings.len()
            ))
        })
}

/// Lowers one IR function into the Cranelift `Context`.
fn define_function(
    ir: &IrModule,
    function: &IrFunction,
    function_name: &str,
    context: &mut Context,
    func_refs: &HashMap<IrSymbol, FuncRef>,
) -> Result<(), CodegenError> {
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut fn_builder_ctx);

    // Create all blocks first so forward references (joins, loop
    // headers, arms) resolve; IR block 0 is the entry block, and the
    // first `switch_to_block` below is on it, which makes it the
    // Cranelift entry.
    let blocks: Vec<Block> = function
        .blocks
        .iter()
        .map(|_| builder.create_block())
        .collect();
    builder.append_block_params_for_function_params(blocks[0]);
    let entry_params: Vec<Value> = builder.block_params(blocks[0]).to_vec();

    let mut state = FnState {
        values: HashMap::new(),
        locals: HashMap::new(),
        defined_locals: HashSet::new(),
        next_variable: 0,
        blocks,
        func_refs,
        strings: &ir.strings,
        function_name,
        returns_int: matches!(function.return_type, IrTypeTag::Int),
    };
    for (index, &param_id) in function.params.iter().enumerate() {
        state.values.insert(param_id, entry_params[index]);
    }

    for block in &function.blocks {
        let target = state.block(block.id)?;
        builder.switch_to_block(target);
        for inst in &block.insts {
            lower_inst(&mut state, &mut builder, inst)?;
        }
        lower_terminator(&mut state, &mut builder, &block.terminator)?;
    }

    // Blocks are filled in IR order while some predecessors (loop
    // back-edges) are emitted after their targets' instructions; the
    // frontend's incomplete-CFG SSA construction resolves locals when
    // everything is sealed here.
    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

/// Per-function lowering state: the value-id → Cranelift value map,
/// the local-symbol → Variable map, and the block table.
struct FnState<'a> {
    values: HashMap<IrValueId, Value>,
    locals: HashMap<IrSymbol, Variable>,
    defined_locals: HashSet<IrSymbol>,
    next_variable: usize,
    blocks: Vec<Block>,
    func_refs: &'a HashMap<IrSymbol, FuncRef>,
    strings: &'a [String],
    function_name: &'a str,
    returns_int: bool,
}

impl<'a> FnState<'a> {
    fn bind_result(&mut self, result: Option<IrValueId>, value: Value) {
        if let Some(id) = result {
            self.values.insert(id, value);
        }
    }

    /// Value ids referenced by operands must have been lowered; a miss
    /// names the value (calls to unit-returning functions produce no
    /// machine value, so using one is a broken program shape).
    fn value(&self, id: IrValueId) -> Result<Value, CodegenError> {
        self.values.get(&id).copied().ok_or_else(|| {
            CodegenError::JitError(format!(
                "value v{id} in `{}` was used but never lowered to a machine value (calls to \
                 unit-returning functions produce no value)",
                self.function_name
            ))
        })
    }

    fn symbol_text(&self, sym: IrSymbol) -> Result<&'a str, CodegenError> {
        let strings: &'a [String] = self.strings;
        strings
            .get(sym as usize)
            .map(String::as_str)
            .ok_or_else(|| {
                CodegenError::JitError(format!(
                    "malformed IR: symbol {sym} is out of range (string table holds {} entries)",
                    strings.len()
                ))
            })
    }

    fn block(&self, id: u32) -> Result<Block, CodegenError> {
        self.blocks.get(id as usize).copied().ok_or_else(|| {
            CodegenError::JitError(format!(
                "malformed IR: branch target {id} is out of range ({} blocks)",
                self.blocks.len()
            ))
        })
    }

    /// A `LoadLocal` whose symbol was never stored is an unbound name:
    /// module-level consts (which lower as `LoadLocal` per the RFC
    /// 0054 MIR note) or a function whose parameters were not bound
    /// with [`bind_function_params`].
    fn local(&self, sym: IrSymbol) -> Result<Variable, CodegenError> {
        if !self.defined_locals.contains(&sym) {
            let name = self.symbol_text(sym)?;
            return Err(CodegenError::UnsupportedConstruct {
                construct: format!(
                    "load of unbound local `{name}` (module-level consts are not lowered in v1; \
                     bind function parameters with `bind_function_params`)"
                ),
            });
        }
        self.locals.get(&sym).copied().ok_or_else(|| {
            CodegenError::JitError(format!(
                "internal: local `{}` in `{}` is marked defined but has no Variable",
                self.symbol_text(sym).unwrap_or("?"),
                self.function_name
            ))
        })
    }

    fn declare_local(&mut self, sym: IrSymbol, builder: &mut FunctionBuilder) -> Variable {
        if let Some(&var) = self.locals.get(&sym) {
            return var;
        }
        let var = Variable::new(self.next_variable);
        self.next_variable += 1;
        builder.declare_var(var, types::I64);
        self.locals.insert(sym, var);
        var
    }

    /// A call to a symbol that is not a declared module function is a
    /// builtin (or unknown) callee — deferred, named honestly.
    fn func_ref(&self, sym: IrSymbol) -> Result<FuncRef, CodegenError> {
        let name = self.symbol_text(sym)?;
        self.func_refs
            .get(&sym)
            .copied()
            .ok_or_else(|| CodegenError::UnsupportedConstruct {
                construct: format!(
                    "call to unknown function `{name}` (builtin calls are not lowered in v1; \
                     only module functions are callable)"
                ),
            })
    }
}

fn lower_inst(
    state: &mut FnState,
    builder: &mut FunctionBuilder,
    inst: &IrInst,
) -> Result<(), CodegenError> {
    match &inst.kind {
        IrInstKind::ConstInt(value) => {
            let lowered = builder.ins().iconst(types::I64, *value);
            state.bind_result(inst.result, lowered);
        }
        IrInstKind::ConstBool(flag) => {
            // bools are i64 0/1 in the v1 value space
            let lowered = builder.ins().iconst(types::I64, *flag as i64);
            state.bind_result(inst.result, lowered);
        }
        IrInstKind::Unary { op, value } => {
            let operand = state.value(*value)?;
            let lowered = match op {
                IrUnaryOp::Neg => builder.ins().ineg(operand),
                IrUnaryOp::Not => {
                    // exact boolean negation in the 0/1 representation
                    let one = builder.ins().iconst(types::I64, 1);
                    builder.ins().bxor(operand, one)
                }
            };
            state.bind_result(inst.result, lowered);
        }
        IrInstKind::Binary { op, lhs, rhs } => {
            let left = state.value(*lhs)?;
            let right = state.value(*rhs)?;
            let lowered = match op {
                IrBinaryOp::Add => builder.ins().iadd(left, right),
                IrBinaryOp::Sub => builder.ins().isub(left, right),
                IrBinaryOp::Mul => builder.ins().imul(left, right),
                IrBinaryOp::Div => builder.ins().sdiv(left, right),
                IrBinaryOp::Mod => builder.ins().srem(left, right),
                // comparisons yield i8; widen into the i64 value space
                IrBinaryOp::Eq => {
                    let flag = builder.ins().icmp(IntCC::Equal, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                IrBinaryOp::Ne => {
                    let flag = builder.ins().icmp(IntCC::NotEqual, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                IrBinaryOp::Lt => {
                    let flag = builder.ins().icmp(IntCC::SignedLessThan, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                IrBinaryOp::Le => {
                    let flag = builder
                        .ins()
                        .icmp(IntCC::SignedLessThanOrEqual, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                IrBinaryOp::Gt => {
                    let flag = builder.ins().icmp(IntCC::SignedGreaterThan, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                IrBinaryOp::Ge => {
                    let flag = builder
                        .ins()
                        .icmp(IntCC::SignedGreaterThanOrEqual, left, right);
                    builder.ins().uextend(types::I64, flag)
                }
                // strict boolean conjunction/disjunction on 0/1 values
                IrBinaryOp::And => builder.ins().band(left, right),
                IrBinaryOp::Or => builder.ins().bor(left, right),
            };
            state.bind_result(inst.result, lowered);
        }
        IrInstKind::LoadLocal { sym } => {
            let var = state.local(*sym)?;
            let lowered = builder.use_var(var);
            state.bind_result(inst.result, lowered);
        }
        IrInstKind::StoreLocal { sym, value, .. } => {
            // `mutable`/`declare` are source-level metadata; machine
            // locals are uniformly mutable slots.
            let operand = state.value(*value)?;
            let var = state.declare_local(*sym, builder);
            builder.def_var(var, operand);
            state.defined_locals.insert(*sym);
        }
        IrInstKind::Call { sym, args } => {
            let func_ref = state.func_ref(*sym)?;
            let mut arguments = Vec::with_capacity(args.len());
            for arg in args {
                arguments.push(state.value(*arg)?);
            }
            let call = builder.ins().call(func_ref, &arguments);
            let results = builder.inst_results(call);
            if let (Some(result_id), Some(&result)) = (inst.result, results.first()) {
                // unit callees produce no result value; using their IR
                // result id later fails honestly in `value()`.
                state.values.insert(result_id, result);
            }
        }
        IrInstKind::Eval { .. } => {
            // statement-position expression: the value's computation
            // (and any side effect) already happened at its own
            // instruction; the discarded value needs no machine code.
        }
        IrInstKind::Spawn { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "`go` task spawn (the concurrency runtime ABI slice)".to_string(),
            });
        }
        IrInstKind::Await { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "`await` (the concurrency runtime ABI slice)".to_string(),
            });
        }
        IrInstKind::Propagate { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "`?` propagation (the Result value-representation slice)".to_string(),
            });
        }
        IrInstKind::ConstString(_) => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "string constants (the string value-representation slice)".to_string(),
            });
        }
        IrInstKind::ConstEnumVariant { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "enum variant constants (the enum value-representation slice)"
                    .to_string(),
            });
        }
        IrInstKind::BuildList(_) => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "list construction (the list value-representation slice)".to_string(),
            });
        }
        IrInstKind::BuildDict(_) => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "dict construction (the dict value-representation slice)".to_string(),
            });
        }
        IrInstKind::BuildStruct { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "struct construction (the struct value-representation slice)"
                    .to_string(),
            });
        }
        IrInstKind::LoadField { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "field access (the struct value-representation slice)".to_string(),
            });
        }
        IrInstKind::Index { .. } => {
            return Err(CodegenError::UnsupportedConstruct {
                construct: "indexing (the container value-representation slice)".to_string(),
            });
        }
    }
    Ok(())
}

fn lower_terminator(
    state: &mut FnState,
    builder: &mut FunctionBuilder,
    terminator: &IrTerminator,
) -> Result<(), CodegenError> {
    match terminator {
        IrTerminator::Jump { block } => {
            let target = state.block(*block)?;
            builder.ins().jump(target, &[]);
        }
        IrTerminator::Branch {
            cond,
            then_block,
            else_block,
        } => {
            let condition = state.value(*cond)?;
            let then_target = state.block(*then_block)?;
            let else_target = state.block(*else_block)?;
            // brif takes an i8 condition; the value space is 0/1
            let flag = builder.ins().ireduce(types::I8, condition);
            builder.ins().brif(flag, then_target, &[], else_target, &[]);
        }
        IrTerminator::MatchDispatch { .. } => {
            // v1 has no enum-tag instruction: the (enum, variant)
            // symbols per arm cannot be tested in machine code. The
            // evaluator remains the execution surface for match.
            return Err(CodegenError::UnsupportedConstruct {
                construct: MATCH_DISPATCH_DEFERRAL.to_string(),
            });
        }
        IrTerminator::Return { value } => match (value, state.returns_int) {
            (Some(id), true) => {
                let result = state.value(*id)?;
                builder.ins().return_(&[result]);
            }
            (None, true) => {
                // Dead-code terminator (the unreachable join after
                // early returns). Trap loudly — never a silent 0.
                builder.ins().trap(DEAD_IMPLICIT_RETURN_TRAP);
            }
            (_, false) => {
                // Unit-returning function: the returned value's
                // computation already happened; the machine signature
                // returns nothing.
                builder.ins().return_(&[]);
            }
        },
        IrTerminator::Unreachable => {
            // The documented deferral regions: for-loop headers,
            // select, invalid break/continue. Honest error, never a
            // silent skip.
            return Err(CodegenError::UnsupportedConstruct {
                construct: UNREACHABLE_DEFERRAL.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CodegenError, bind_function_params, execute};
    use crate::backend_ir::{self, IrInstKind, IrTerminator};
    use crate::hir::lower as lower_hir;
    use crate::mir::lower as lower_mir;
    use crate::module_graph::parse_single_source;
    use crate::source::SourceFile;
    use crate::ssa::{SsaModule, lower as lower_ssa};
    use crate::typed_hir::lower as lower_typed;

    /// The REAL pipeline chain the `run_module_machine_code` surface
    /// uses (minus the module-graph/provider layer these
    /// import-free fixtures do not need — the same shape
    /// `backend_ir`'s tests use).
    fn ssa_from_source(path: &str, source: &str) -> SsaModule {
        let file = SourceFile::new(path, source);
        let ast = parse_single_source(&file).expect("parse should succeed");
        let hir = lower_hir(&ast);
        let (typed, _warnings) = lower_typed(&hir).expect("typing should succeed");
        let mir = lower_mir(&typed);
        lower_ssa(&mir)
    }

    fn machine_code_result(path: &str, source: &str) -> Result<i64, CodegenError> {
        let ssa = ssa_from_source(path, source);
        let ir = backend_ir::lower(&ssa);
        let param_names: Vec<Vec<String>> = ssa
            .functions
            .iter()
            .map(|function| function.params.clone())
            .collect();
        let bound = bind_function_params(&ir, &param_names)?;
        execute(&bound)
    }

    fn expect_machine_code(path: &str, source: &str, expected: i64) {
        let actual = machine_code_result(path, source)
            .unwrap_or_else(|error| panic!("machine execution should succeed: {error}"));
        assert_eq!(actual, expected, "machine-computed value for {path}");
    }

    // -- the smoke scope: real machine code ----------------------------------

    #[test]
    fn int_arithmetic_runs_as_machine_code() {
        expect_machine_code("arith.gof", "fn main() -> int:\n    return 2 + 3 * 4\n", 14);
    }

    #[test]
    fn division_modulo_and_negation_run_as_machine_code() {
        expect_machine_code(
            "numeric.gof",
            "fn main() -> int:\n    total = 17 % 5\n    quotient = 17 / 5\n    negated = -3\n    return total + quotient - negated\n",
            // 2 + 3 - (-3) = 8
            8,
        );
    }

    #[test]
    fn if_else_branches_run_as_machine_code() {
        expect_machine_code(
            "classify_pos.gof",
            "fn classify(n: int) -> int:\n    if n > 0:\n        return 1\n    else:\n        return 0\n\nfn main() -> int:\n    return classify(5)\n",
            1,
        );
        expect_machine_code(
            "classify_neg.gof",
            "fn classify(n: int) -> int:\n    if n > 0:\n        return 1\n    else:\n        return 0\n\nfn main() -> int:\n    return classify(-3)\n",
            0,
        );
    }

    #[test]
    fn while_loop_runs_as_machine_code() {
        // The key proof: the IR's CONDITION-MOVE header re-evaluates
        // `total < limit` every iteration, and the loop-carried local
        // flows through the machine-code loop correctly.
        expect_machine_code(
            "count.gof",
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n\nfn main() -> int:\n    return count(50)\n",
            50,
        );
    }

    #[test]
    fn recursion_runs_as_machine_code() {
        expect_machine_code(
            "depth.gof",
            "fn depth(n: int) -> int:\n    if n == 0:\n        return 0\n    else:\n        return depth(n - 1) + 1\n\nfn main() -> int:\n    return depth(100)\n",
            100,
        );
    }

    #[test]
    fn function_parameters_bind_by_position_not_load_order() {
        // The body loads `b` before `a`, so any load-order binding
        // would compute 5; only positional argument binding gives
        // 4 - 9 = -5.
        expect_machine_code(
            "sub.gof",
            "fn sub(a: int, b: int) -> int:\n    return b - a\n\nfn main() -> int:\n    return sub(9, 4)\n",
            -5,
        );
    }

    #[test]
    fn calls_with_multiple_arguments_run_as_machine_code() {
        expect_machine_code(
            "combine.gof",
            "fn combine(a: int, b: int, c: int) -> int:\n    return a * 100 + b * 10 + c\n\nfn main() -> int:\n    return combine(1, 2, 3)\n",
            123,
        );
    }

    #[test]
    fn bool_logic_and_bool_parameters_run_as_machine_code() {
        expect_machine_code(
            "gate.gof",
            "fn gate(flag: bool, n: int) -> int:\n    if flag and n > 0:\n        return 1\n    else:\n        return 0\n\nfn main() -> int:\n    return gate(true, 5) * 10 + gate(false, 5)\n",
            10,
        );
    }

    #[test]
    fn bool_not_and_local_bindings_run_as_machine_code() {
        expect_machine_code(
            "not_flag.gof",
            "fn main() -> int:\n    value = 5\n    flag = value > 10\n    if not flag:\n        return 1\n    return 0\n",
            1,
        );
    }

    // -- honest deferrals ------------------------------------------------------

    #[test]
    fn match_program_fails_honestly() {
        let error = machine_code_result(
            "match.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 10\n        Status.Busy:\n            return 20\n\nfn main() -> int:\n    status: Status = Status.Busy\n    return score(status)\n",
        )
        .expect_err("match dispatch must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert_eq!(construct, "match dispatch");
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    #[test]
    fn string_returning_function_fails_honestly() {
        let error = machine_code_result(
            "label.gof",
            "fn label() -> string:\n    return \"big\"\n\nfn main() -> int:\n    return 1\n",
        )
        .expect_err("a string return type must be an honest error");
        match error {
            CodegenError::UnsupportedType { function, ty } => {
                assert_eq!(function, "label");
                assert_eq!(ty, "String");
            }
            other => panic!("expected UnsupportedType, got {other}"),
        }
    }

    #[test]
    fn select_region_fails_honestly_as_unreachable_deferral() {
        let error = machine_code_result(
            "select.gof",
            "fn main() -> int:\n    select:\n        default:\n            return 1\n",
        )
        .expect_err("the select deferral must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(
                    construct.contains("unreachable terminator"),
                    "the deferral must name the unreachable terminator: {construct}"
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    #[test]
    fn for_loop_fails_honestly() {
        // The iterable's BuildList errors first (lists are deferred);
        // the for-loop header's Unreachable deferral sits behind it.
        let error = machine_code_result(
            "for_loop.gof",
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return total\n",
        )
        .expect_err("for loops must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(
                    construct.contains("list"),
                    "for-over-list names the list deferral: {construct}"
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    #[test]
    fn missing_main_fails_honestly() {
        let error = machine_code_result("helper_only.gof", "fn helper() -> int:\n    return 1\n")
            .expect_err("no main must be an honest error");
        assert_eq!(error, CodegenError::MissingMain);
    }

    #[test]
    fn main_with_parameters_fails_honestly() {
        let error = machine_code_result(
            "main_params.gof",
            "fn main(seed: int) -> int:\n    return seed\n",
        )
        .expect_err("main with parameters must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(
                    construct.contains("`main` with parameters"),
                    "got: {construct}"
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    #[test]
    fn main_with_non_int_return_fails_honestly() {
        let error = machine_code_result("main_bool.gof", "fn main() -> bool:\n    return true\n")
            .expect_err("bool main must be an honest error");
        match error {
            CodegenError::UnsupportedType { function, ty } => {
                assert_eq!(function, "main");
                assert_eq!(ty, "Bool");
            }
            other => panic!("expected UnsupportedType, got {other}"),
        }
    }

    #[test]
    fn const_reference_fails_honestly_as_unbound_local() {
        // RFC 0054: const references lower as LoadLocal of the const
        // name; nothing binds it, so v1 names it honestly.
        let error = machine_code_result(
            "const_ref.gof",
            "const LIMIT: int = 10\n\nfn main() -> int:\n    return LIMIT\n",
        )
        .expect_err("const references must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(
                    construct.contains("unbound local `LIMIT`"),
                    "the const deferral must name the unbound local: {construct}"
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    #[test]
    fn builtin_call_fails_honestly_as_unknown_function() {
        let error = machine_code_result(
            "print_call.gof",
            "fn main() -> int:\n    print(1)\n    return 0\n",
        )
        .expect_err("builtin calls must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(
                    construct.contains("call to unknown function `print`"),
                    "builtin calls must name the callee: {construct}"
                );
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    // -- parameter binding contract -------------------------------------------

    #[test]
    fn bind_function_params_produces_valid_self_contained_ir() {
        let ssa = ssa_from_source(
            "binding.gof",
            "fn sub(a: int, b: int) -> int:\n    return b - a\n\nfn main() -> int:\n    return sub(9, 4)\n",
        );
        let ir = backend_ir::lower(&ssa);
        let param_names: Vec<Vec<String>> = ssa
            .functions
            .iter()
            .map(|function| function.params.clone())
            .collect();
        let bound =
            bind_function_params(&ir, &param_names).expect("parameter binding should succeed");
        // The bound module is ordinary, valid IR: the entry block's
        // first instructions bind the param value ids to their locals.
        backend_ir::validate(&bound).expect("bound IR must validate");
        let sub = &bound.functions[0];
        match &sub.blocks[0].insts[0].kind {
            IrInstKind::StoreLocal { sym, value, .. } => {
                assert_eq!(bound.strings[*sym as usize], "a");
                assert_eq!(*value, sub.params[0], "param 0's id binds to local `a`");
            }
            other => panic!("expected the injected param StoreLocal, got {other:?}"),
        }
    }

    #[test]
    fn bind_function_params_rejects_mismatched_tables() {
        let ssa = ssa_from_source("mismatch.gof", "fn main() -> int:\n    return 1\n");
        let ir = backend_ir::lower(&ssa);
        let mismatched = vec![vec!["a".to_string(), "b".to_string()]];
        match bind_function_params(&ir, &mismatched) {
            Err(CodegenError::JitError(detail)) => {
                assert!(detail.contains("mismatch"), "got: {detail}");
            }
            other => panic!("expected a JitError mismatch, got {other:?}"),
        }
    }

    #[test]
    fn execute_requires_bound_parameters() {
        // Without bind_function_params, the body's LoadLocal of a
        // parameter is an unbound local — never a silent wrong value.
        let ssa = ssa_from_source(
            "unbound.gof",
            "fn id(n: int) -> int:\n    return n\n\nfn main() -> int:\n    return id(7)\n",
        );
        let ir = backend_ir::lower(&ssa);
        let error = execute(&ir).expect_err("unbound params must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert!(construct.contains("unbound local `n`"), "got: {construct}");
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    // -- the pipeline surface ---------------------------------------------------

    #[test]
    fn pipeline_run_module_machine_code_executes_int_programs() {
        let source = SourceFile::new(
            "pipeline_arith.gof",
            "fn main() -> int:\n    return 2 + 3 * 4\n",
        );
        let value = crate::pipeline::run_module_machine_code(&source)
            .expect("the machine-code pipeline should run the smoke program");
        assert_eq!(value, 14);
    }

    #[test]
    fn pipeline_run_module_machine_code_maps_gaps_to_gof3180() {
        let source = SourceFile::new(
            "pipeline_match.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 10\n        Status.Busy:\n            return 20\n\nfn main() -> int:\n    status: Status = Status.Busy\n    return score(status)\n",
        );
        let diagnostics = crate::pipeline::run_module_machine_code(&source)
            .expect_err("match programs must surface a GOF3180 diagnostic");
        assert_eq!(diagnostics.0.len(), 1);
        let diagnostic = &diagnostics.0[0];
        assert_eq!(diagnostic.code, "GOF3180");
        assert_eq!(
            diagnostic.message,
            "native execution does not yet support this construct"
        );
        assert!(
            diagnostic.note.contains("match dispatch"),
            "the note must name the construct: {}",
            diagnostic.note
        );
    }

    #[test]
    fn pipeline_run_module_machine_code_reports_missing_main() {
        let source = SourceFile::new(
            "pipeline_no_main.gof",
            "fn helper() -> int:\n    return 1\n",
        );
        let diagnostics = crate::pipeline::run_module_machine_code(&source)
            .expect_err("a module without main must surface a diagnostic");
        assert_eq!(diagnostics.0[0].code, "GOF3180");
        assert!(
            diagnostics.0[0].note.contains("fn main"),
            "the note must name the missing entry point: {}",
            diagnostics.0[0].note
        );
    }

    // -- shape checks through the real lowering ---------------------------------

    #[test]
    fn while_loop_ir_shape_is_the_condition_move_header() {
        // Structural companion to while_loop_runs_as_machine_code:
        // the condition instructions live in the Branch block, and the
        // back-edge targets it.
        let ssa = ssa_from_source(
            "while_shape.gof",
            "fn count(limit: int) -> int:\n    mut total = 0\n    while total < limit:\n        total = total + 1\n    return total\n",
        );
        let ir = backend_ir::lower(&ssa);
        let count = &ir.functions[0];
        let header = count
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, IrTerminator::Branch { .. }))
            .expect("the while header must be a Branch block");
        let cond = match &header.terminator {
            IrTerminator::Branch { cond, .. } => *cond,
            other => panic!("expected a Branch terminator, got {other:?}"),
        };
        assert!(
            header.insts.iter().any(|inst| inst.result == Some(cond)),
            "the loop condition must be computed in the header"
        );
    }
}
