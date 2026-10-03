//! Phase 2 dev-build budget benchmark (ADR 0004's "benchmark-pinned"
//! validation gate): the dev-build wall-clock cost of the canonical
//! int/bool program, measured on the FIXED fixture
//! (`fixtures/dev_budget.gof` — deterministic, `gof fmt`-clean). The
//! measurement helpers live in the crate lib so the gate
//! (`tests/dev_build_budget_gate.rs`) enforces exactly what this
//! benchmark measures.
//!
//! The measurements, all through public pipeline seams:
//!
//! - **frontend** — `compile_source` (lex → CST → HIR → typed HIR →
//!   MIR → SSA → backend artifact): the stages the IR cache skips on
//!   a hit.
//! - **pipeline uncached** — frontend + backend-IR lowering + param
//!   binding + Cranelift object emission. This is the PINNED
//!   uncached dev-build budget: the compiler's own work.
//! - **pipeline cache hit** — the incremental path a hit actually
//!   runs: module-graph load + content key + entry decode + object
//!   emission. This is the PINNED rebuild budget.
//!
//! The link step is EXCLUDED from the pinned budgets, on purpose.
//! Linking spawns external processes — `probe_linkers()` shells out
//! to `rustc --print sysroot` before spawning the linker itself —
//! which is machine state, not compiler state (the cache contract
//! says the same: a no-linker machine still populates the cache). On
//! the reference machine the link-inclusive full build measured
//! 1.2–1.7 s with run-to-run swings larger than the entire in-process
//! pipeline (milliseconds), and repeated linker spawning even
//! hard-crashed the bench binary intermittently (antivirus/process
//! interference) — unusable as a pinned number, and unsafe for the
//! `cargo bench` quality gate. The link-inclusive context numbers
//! (criterion runs and CLI wall-clock) are recorded in
//! [BUDGETS.md](../BUDGETS.md) with a re-measure procedure that does
//! not gate anything.
//!
//! The gate that ENFORCES the budget is
//! `tests/dev_build_budget_gate.rs` (an opt-in `#[ignore]` test; the
//! pinned numbers and the re-measure procedure live in
//! [BUDGETS.md](../BUDGETS.md)).

use criterion::{Criterion, criterion_group, criterion_main};
use gof_bench::{
    dev_build_cache_hit_object, dev_build_frontend, dev_build_scratch_dir,
    dev_build_store_cache_entry, dev_build_uncached_object,
};
use gof_compiler::backend_cache::IrCache;
use std::hint::black_box;

/// The frontend-only measurement: everything the IR cache skips on a
/// hit (parse → typed HIR → MIR → SSA → backend artifact).
fn benchmark_frontend(c: &mut Criterion) {
    c.bench_function("dev build frontend parse to ssa", |bench| {
        bench.iter(|| black_box(dev_build_frontend().ssa.functions.len()));
    });
}

/// The PINNED uncached dev-build pipeline: frontend → backend IR →
/// param binding → Cranelift object emission. No link.
fn benchmark_pipeline_uncached(c: &mut Criterion) {
    c.bench_function("dev build pipeline uncached", |bench| {
        bench.iter(|| black_box(dev_build_uncached_object()));
    });
}

/// The PINNED cache-hit (incremental rebuild) pipeline: graph load →
/// content key → entry decode → object emission. No link. The cache
/// entry is stored once up front — exactly what a MISS build stores —
/// so every timed iteration is a HIT.
fn benchmark_pipeline_cache_hit(c: &mut Criterion) {
    let cache = IrCache::new(dev_build_scratch_dir("pipeline-cache-hit"));
    dev_build_store_cache_entry(&cache);

    c.bench_function("dev build pipeline cache hit", |bench| {
        bench.iter(|| black_box(dev_build_cache_hit_object(&cache)));
    });
}

criterion_group!(
    benches,
    benchmark_frontend,
    benchmark_pipeline_uncached,
    benchmark_pipeline_cache_hit,
);
criterion_main!(benches);
