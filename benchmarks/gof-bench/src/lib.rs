//! Shared surface for the gof benchmark crate: the canonical fixture
//! programs benches measure, the pinned Phase 2 dev-build budget, and
//! the measurement helpers both the benchmark
//! (`benches/dev_build_budget.rs`) and the gate
//! (`tests/dev_build_budget_gate.rs`) share — the gate enforces
//! exactly what the benchmark measures, with zero drift by
//! construction.
//!
//! The budget record — the measured numbers, the machine, the
//! re-measure and re-pin procedure — lives in
//! [BUDGETS.md](../BUDGETS.md).

use gof_compiler::backend_cache::{CacheSource, IrCache, cache_key};
use gof_compiler::backend_codegen::bind_function_params;
use gof_compiler::backend_ir::lower as lower_backend_ir;
use gof_compiler::backend_object::emit_object;
use gof_compiler::module_graph::load_module_graph_with_provider;
use gof_compiler::pipeline::CompiledModule;
use gof_compiler::{CompileMode, FileSystemSourceProvider, SourceFile, compile_source};
use std::path::PathBuf;

/// The canonical dev-build budget fixture (ADR 0004's Phase 2 kickoff
/// gate, [BUDGETS.md](../BUDGETS.md)): a 90-line int/bool program —
/// eleven functions, nested `while` loops, `if`/`else` chains, calls
/// with bool arguments, `and`/`or`/`not` logic, and the full int
/// operator set. Exactly the v1 machine-code surface: the program
/// builds through `gof build --native-object` (exit code 163 on both
/// the evaluator and the native binary, the emitted object
/// byte-identical between cached and uncached builds), and it stays
/// `gof fmt`-clean. Deterministic by construction: every value is a
/// pure function of the literals in the file.
pub fn dev_budget_source() -> &'static str {
    include_str!("../fixtures/dev_budget.gof")
}

/// The pinned Phase 2 dev-build budgets — the executable form of the
/// record in [BUDGETS.md](../BUDGETS.md). Pinned on the reference
/// machine (see the record for its identity and date) under the
/// release/bench profile, through the public pipeline seams, with the
/// link step EXCLUDED (linking spawns external processes — machine
/// state, not compiler state; the record explains the decision and
/// carries the link-inclusive context numbers).
///
/// Re-pinning is a reviewed decision, never a side effect: change the
/// constant here, re-measure with `cargo bench -p gof-bench -- "dev
/// build"`, and update the record.
pub mod dev_build_budget {
    /// The UNCACHED dev-build pipeline of the canonical fixture
    /// (frontend → backend IR → param binding → Cranelift object
    /// emission, no link): ≤ 4.0 ms.
    pub const UNCACHED_PIPELINE_MS: f64 = 4.0;

    /// The cache-hit (incremental rebuild) pipeline — what a
    /// `--cache` HIT actually runs (module-graph load + content key +
    /// entry decode + object emission, no link): ≤ 3.0 ms.
    pub const CACHE_HIT_PIPELINE_MS: f64 = 3.0;

    /// The regression margin the gate tripwire tolerates before
    /// failing: 3× the budget. Deliberately generous — the gate is a
    /// tripwire against real regressions, not a precision instrument;
    /// 3× absorbs measurement noise while still catching the
    /// 10×-class regressions that actually eat a dev budget.
    pub const GATE_MARGIN: f64 = 3.0;
}

/// The fixture's source name (diagnostics only — the fixture has no
/// imports, so nothing ever touches the filesystem for it).
const FIXTURE_NAME: &str = "dev_budget.gof";

/// The fixture as an in-memory `SourceFile` — the exact fixed bytes
/// the bench and the gate both build.
pub fn dev_budget_source_file() -> SourceFile {
    SourceFile::new(FIXTURE_NAME, dev_budget_source())
}

/// A scratch directory under the system temp dir for bench/gate
/// artifacts (cache entries never touch the repository tree).
pub fn dev_build_scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("gof-bench-dev-build-budget")
        .join(tag);
    std::fs::create_dir_all(&dir).expect("the scratch directory must be creatable");
    dir
}

/// The param-name lists threaded from SSA — the same extraction
/// `pipeline::build_module_native_executable` performs before IR
/// lowering.
fn param_names(compiled: &CompiledModule) -> Vec<Vec<String>> {
    compiled
        .ssa
        .functions
        .iter()
        .map(|function| function.params.clone())
        .collect()
}

/// One FRONTEND build of the fixture: `compile_source` (lex → CST →
/// HIR → typed HIR → MIR → SSA → backend artifact). Diagnostic
/// measurement — these are exactly the stages the IR cache skips on a
/// hit, so this number is the cache's theoretical ceiling of savings.
pub fn dev_build_frontend() -> CompiledModule {
    compile_source(&dev_budget_source_file(), CompileMode::Executable)
        .expect("the budget fixture must compile")
}

/// One UNCACHED in-process dev-build of the fixture: frontend →
/// backend-IR lowering → param binding → Cranelift object emission.
/// The object bytes are the same deterministic artifact
/// `gof build --native-object` produces (before linking). This is
/// what the pinned uncached budget measures.
pub fn dev_build_uncached_object() -> Vec<u8> {
    let compiled = dev_build_frontend();
    let ir = lower_backend_ir(&compiled.ssa);
    let bound = bind_function_params(&ir, &param_names(&compiled))
        .expect("the budget fixture must bind params");
    emit_object(&bound).expect("the budget fixture must emit an object")
}

/// The cache key the public cached build derives for the fixture's
/// module graph (the same derivation
/// `pipeline::build_module_native_executable_cached` performs).
fn dev_build_cache_key() -> String {
    let graph =
        load_module_graph_with_provider(&dev_budget_source_file(), &FileSystemSourceProvider)
            .expect("the fixture's module graph must load");
    let sources: Vec<CacheSource<'_>> = graph
        .sources
        .iter()
        .map(|graph_source| CacheSource {
            path: graph_source.path(),
            bytes: graph_source.text().as_bytes(),
        })
        .collect();
    cache_key(&sources)
}

/// Stores the cache entry a MISS build stores (the param-bound module
/// — the complete codegen input, per the cache contract). After this,
/// every [`dev_build_cache_hit_object`] call runs the incremental
/// path.
pub fn dev_build_store_cache_entry(cache: &IrCache) {
    let compiled = dev_build_frontend();
    let ir = lower_backend_ir(&compiled.ssa);
    let bound = bind_function_params(&ir, &param_names(&compiled))
        .expect("the budget fixture must bind params");
    cache.store(&dev_build_cache_key(), &bound);
}

/// One cache-HIT in-process incremental rebuild of the fixture: the
/// module-graph load + content key + entry decode + object emission a
/// `--cache` HIT actually runs (the link step excluded). This is what
/// the pinned cache-hit budget measures. Panics loudly if the cache
/// does not hit — a miss here means the key derivation or the cache
/// contract drifted, which is exactly what a gate must not swallow.
pub fn dev_build_cache_hit_object(cache: &IrCache) -> Vec<u8> {
    let source = dev_budget_source_file();
    let graph = load_module_graph_with_provider(&source, &FileSystemSourceProvider)
        .expect("the fixture's module graph must load");
    let sources: Vec<CacheSource<'_>> = graph
        .sources
        .iter()
        .map(|graph_source| CacheSource {
            path: graph_source.path(),
            bytes: graph_source.text().as_bytes(),
        })
        .collect();
    let key = cache_key(&sources);
    let bound = cache
        .lookup(&key)
        .expect("the warmed cache entry must hit (a miss means the key derivation drifted)");
    emit_object(&bound).expect("the cached IR must emit an object")
}
