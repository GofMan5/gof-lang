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

/// The concurrency runtime measurement surface (Phase 3, audit P7 +
/// P8): the canonical fixture programs the concurrency benches
/// measure, the round counts that turn per-program wall-clock into
/// per-operation rates, and the compile/run helpers the benchmark
/// (`benches/concurrency_runtime.rs`) and the gates
/// (`tests/concurrency_stress_gate.rs`) share — the gates enforce
/// exactly what the benchmark measures, with zero drift by
/// construction: every gate's expected value is computed from the
/// SAME round constant the benchmark's throughput uses, so a fixture
/// edited out of sync with its constant fails the gate loudly
/// instead of silently mislabeling the measurement.
///
/// The measurement record — the numbers, the machine, the
/// interpretation of what each number isolates, and the re-measure
/// procedure — lives in [MEASUREMENTS.md](../MEASUREMENTS.md).
///
/// All fixtures run through the PUBLIC seam (`compile_source` +
/// `run_with_output`), like every other bench in this crate: the
/// interpreter's task/channel/token primitives are private to
/// `gof-compiler`, so the concurrency numbers are end-to-end program
/// runs, and the record documents what each program's structure
/// isolates (the wake-latency fixtures exchange values in a loop so
/// the per-operation number is the run time divided by the loop
/// count, with interpreter setup amortized).
pub mod concurrency_runtime {
    use gof_compiler::interpreter::run_with_output;
    use gof_compiler::pipeline::CompiledModule;
    use gof_compiler::{CompileMode, SourceFile, compile_source};

    /// Rounds in the spawn/join fixture: one `go trivial()` + `await`
    /// round trip per round. Must equal the `range(...)` bound in
    /// [`fixtures/concurrency_spawn_join.gof`](../fixtures/concurrency_spawn_join.gof);
    /// the gate enforces the pairing through the expected value.
    pub const SPAWN_JOIN_ROUNDS: u64 = 128;

    /// Buffered send/recv pairs in the channel-throughput fixture.
    /// Must equal the `range(...)` bound in
    /// [`fixtures/concurrency_channel_pairs.gof`](../fixtures/concurrency_channel_pairs.gof).
    pub const CHANNEL_PAIRS: u64 = 1024;

    /// Rendezvous exchanges in the wake-latency fixture: one
    /// ping/pong round trip (a blocked recv woken by a send, in both
    /// directions) per exchange. Must equal the `range(...)` bounds
    /// in
    /// [`fixtures/concurrency_rendezvous_wake.gof`](../fixtures/concurrency_rendezvous_wake.gof).
    pub const RENDEZVOUS_EXCHANGES: u64 = 512;

    /// Cancel-wake rounds in the cancel-wake fixture: one blocked
    /// recv woken by a token cancel (plus its spawn and join) per
    /// round. Must equal the `range(...)` bound in
    /// [`fixtures/concurrency_cancel_wake.gof`](../fixtures/concurrency_cancel_wake.gof).
    pub const CANCEL_WAKE_ROUNDS: u64 = 128;

    /// Tasks in the 10k stress fixture. Must equal the `range(...)`
    /// bound in
    /// [`fixtures/concurrency_stress_10k.gof`](../fixtures/concurrency_stress_10k.gof).
    pub const STRESS_TASKS: u64 = 10_000;

    fn fixture_module(name: &str, source: &str) -> CompiledModule {
        compile_source(&SourceFile::new(name, source), CompileMode::Executable)
            .expect("the concurrency fixture must compile")
    }

    fn run_module_text(module: &CompiledModule) -> String {
        let result = run_with_output(&module.ast).expect("the concurrency fixture must run");
        result
            .value
            .cli_text()
            .unwrap_or_else(|| "unit".to_string())
    }

    /// The spawn/join fixture compiled once — the trivial task
    /// (`go trivial()` + `await`, rendezvous-free) round trip the
    /// benchmark iterates.
    pub fn spawn_join_module() -> CompiledModule {
        fixture_module(
            "concurrency_spawn_join.gof",
            include_str!("../fixtures/concurrency_spawn_join.gof"),
        )
    }

    /// One run of the spawn/join fixture: `SPAWN_JOIN_ROUNDS`
    /// sequential go+await round trips.
    pub fn run_spawn_join(module: &CompiledModule) -> String {
        run_module_text(module)
    }

    /// The value a correct run of the spawn/join fixture must return
    /// (`trivial` returns 7): the gate's completion assert.
    pub fn spawn_join_expected() -> String {
        (SPAWN_JOIN_ROUNDS * 7).to_string()
    }

    /// The channel-throughput fixture compiled once — buffered
    /// (capacity 64) send/recv pairs, sender and receiver on the same
    /// thread, `CHANNEL_PAIRS` pairs per run.
    pub fn channel_pairs_module() -> CompiledModule {
        fixture_module(
            "concurrency_channel_pairs.gof",
            include_str!("../fixtures/concurrency_channel_pairs.gof"),
        )
    }

    /// One run of the channel-throughput fixture.
    pub fn run_channel_pairs(module: &CompiledModule) -> String {
        run_module_text(module)
    }

    /// The value a correct run of the channel-throughput fixture must
    /// return (the sum of the sent values): the gate's completion
    /// assert.
    pub fn channel_pairs_expected() -> String {
        format!(
            "Result.Ok(value: {})",
            CHANNEL_PAIRS * (CHANNEL_PAIRS - 1) / 2
        )
    }

    /// The rendezvous wake fixture compiled once — two tasks exchange
    /// values over a pair of rendezvous channels; every exchange is a
    /// blocked recv woken by a send (in both directions), so the
    /// per-exchange latency is the run time divided by
    /// `RENDEZVOUS_EXCHANGES`.
    pub fn rendezvous_wake_module() -> CompiledModule {
        fixture_module(
            "concurrency_rendezvous_wake.gof",
            include_str!("../fixtures/concurrency_rendezvous_wake.gof"),
        )
    }

    /// One run of the rendezvous wake fixture.
    pub fn run_rendezvous_wake(module: &CompiledModule) -> String {
        run_module_text(module)
    }

    /// The value a correct run of the rendezvous wake fixture must
    /// return (main's and echo's sums are equal): the gate's
    /// completion assert.
    pub fn rendezvous_wake_expected() -> String {
        format!(
            "Result.Ok(value: {})",
            RENDEZVOUS_EXCHANGES * (RENDEZVOUS_EXCHANGES - 1)
        )
    }

    /// The cancel-wake fixture compiled once — each round spawns a
    /// waiter blocked on `recv(data, token)`, main learns the waiter
    /// is committed through a `ready` channel, cancels the token, and
    /// joins: the same wake the interpreter's default-suite
    /// cancellation tests assert, measured per round.
    pub fn cancel_wake_module() -> CompiledModule {
        fixture_module(
            "concurrency_cancel_wake.gof",
            include_str!("../fixtures/concurrency_cancel_wake.gof"),
        )
    }

    /// One run of the cancel-wake fixture: `CANCEL_WAKE_ROUNDS`
    /// cancel-wake rounds (each including its spawn and join).
    pub fn run_cancel_wake(module: &CompiledModule) -> String {
        run_module_text(module)
    }

    /// The value a correct run of the cancel-wake fixture must return
    /// (each cancelled waiter contributes its ready signal): the
    /// gate's completion assert.
    pub fn cancel_wake_expected() -> String {
        format!("Result.Ok(value: {})", CANCEL_WAKE_ROUNDS)
    }

    /// The 10k stress fixture compiled once — `STRESS_TASKS` spawned
    /// tasks, all joined sequentially, sum asserted.
    pub fn stress_10k_module() -> CompiledModule {
        fixture_module(
            "concurrency_stress_10k.gof",
            include_str!("../fixtures/concurrency_stress_10k.gof"),
        )
    }

    /// One run of the 10k stress fixture: spawn 10k tasks, join all
    /// of them, total the results.
    pub fn run_stress_10k(module: &CompiledModule) -> String {
        run_module_text(module)
    }

    /// The value a correct run of the 10k stress fixture must return
    /// (the sum of `inc(i)` over all tasks): the stress gate's
    /// completion assert — a task lost to a spawn failure or a lost
    /// join changes the sum.
    pub fn stress_10k_expected() -> String {
        format!(
            "Result.Ok(value: {})",
            STRESS_TASKS * (STRESS_TASKS + 1) / 2
        )
    }

    /// The pinned wake-latency budget (audit P7's latency claim,
    /// measured instead of wall-clock-asserted): the per-exchange
    /// rendezvous wake round trip — a blocked `recv` woken by a
    /// `send`, in both directions — through the public interpreter
    /// seam. Pinned on the reference machine (see
    /// [MEASUREMENTS.md](../MEASUREMENTS.md) for identity, date, and
    /// the three-run stability evidence) under the bench/release
    /// profile.
    ///
    /// Re-pinning is a reviewed decision, never a side effect: change
    /// the constant here, re-measure with
    /// `cargo bench -p gof-bench --bench concurrency_runtime --
    /// "rendezvous wake"`, and update the record.
    pub mod wake_budget {
        /// The per-exchange rendezvous wake round-trip budget: ≤
        /// 10.0 µs (measured medians 6.07–6.59 µs across three runs).
        /// The wake claim this pins is ORDERS OF MAGNITUDE, not
        /// precision: a polling-floor regression lands at tens of
        /// milliseconds per exchange — thousands of times this
        /// budget.
        pub const RENDEZVOUS_WAKE_EXCHANGE_US: f64 = 10.0;

        /// The regression margin the gate tripwire tolerates before
        /// failing: 3x the budget, the same deliberately generous
        /// margin the dev-build budget gate uses — absorbs machine
        /// noise while still catching the 1000x-class regressions
        /// (a polling floor) that would actually destroy the
        /// claim.
        pub const GATE_MARGIN: f64 = 3.0;
    }
}
