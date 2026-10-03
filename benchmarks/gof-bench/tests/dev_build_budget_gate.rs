//! The Phase 2 dev-build budget TRIPWIRE: the enforcing side of the
//! budget record ([BUDGETS.md](../BUDGETS.md)). Re-measures the
//! canonical fixture's dev-build pipeline through the same helpers
//! the benchmark uses and FAILS LOUDLY when the median exceeds
//! 3× the pinned budget — a regression margin generous enough to
//! absorb measurement noise, tight enough to catch the 10×-class
//! regressions that actually eat a dev budget. This is a tripwire,
//! not a precision gate.
//!
//! Opt-in on purpose: wall-clock assertions never run in the default
//! test suite — these tests are `#[ignore]`d and must be invoked
//! explicitly, in the RELEASE profile (the budget was pinned under
//! the bench profile; a debug build is an order of magnitude slower
//! and the gate refuses it with instructions):
//!
//! ```text
//! cargo test --release -p gof-bench --test dev_build_budget_gate -- --ignored --nocapture
//! ```
//!
//! On failure: the compiler's dev-build pipeline regressed past the
//! budget's regression margin. Per CONTRIBUTING rule 8, performance
//! regressions are release blockers unless explicitly justified and
//! accepted — fix it or re-pin (see BUDGETS.md's re-pin procedure;
//! re-pinning is a reviewed decision recorded in the budget file, not
//! a silent constant edit).

use gof_bench::dev_build_budget::{CACHE_HIT_PIPELINE_MS, GATE_MARGIN, UNCACHED_PIPELINE_MS};
use gof_bench::{
    dev_build_cache_hit_object, dev_build_scratch_dir, dev_build_store_cache_entry,
    dev_build_uncached_object,
};
use gof_compiler::backend_cache::IrCache;
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Discarded runs before measurement (allocator, code pages, file
/// caches — not part of the budget).
const WARMUP_ITERATIONS: usize = 3;

/// Measured runs; the MEDIAN (noise-robust) is compared against the
/// budget. Small on purpose: each run is a full dev-build, and the
/// 3× margin does the noise absorption.
const MEASURE_ITERATIONS: usize = 15;

fn refuse_debug_profile(gate: &str) {
    if cfg!(debug_assertions) {
        panic!(
            "the {gate} budget gate must run in the RELEASE profile: the budget was pinned under \
             the bench profile and a debug build is an order of magnitude slower.\n  help: cargo \
             test --release -p gof-bench --test dev_build_budget_gate -- --ignored --nocapture"
        );
    }
}

fn median(durations: &mut [Duration]) -> Duration {
    durations.sort();
    durations[durations.len() / 2]
}

/// Measures `build` and asserts its median stays within
/// `GATE_MARGIN`×`budget_ms`.
fn assert_within_budget(gate: &str, budget_ms: f64, build: impl Fn() -> Vec<u8>) {
    for _ in 0..WARMUP_ITERATIONS {
        black_box(build());
    }
    let mut durations: Vec<Duration> = Vec::with_capacity(MEASURE_ITERATIONS);
    for _ in 0..MEASURE_ITERATIONS {
        let start = Instant::now();
        black_box(build());
        durations.push(start.elapsed());
    }

    let median = median(&mut durations);
    let threshold = Duration::from_secs_f64(budget_ms / 1000.0 * GATE_MARGIN);
    eprintln!(
        "{gate}: median {:.2?} (min {:.2?}, max {:.2?}) against a {:.1} ms budget — tripwire at \
         {:.2?}",
        median,
        durations[0],
        durations[durations.len() - 1],
        budget_ms,
        threshold
    );
    assert!(
        median <= threshold,
        "the {gate} dev-build pipeline regressed past the budget's {GATE_MARGIN}x regression \
         margin: median {median:.2?} > tripwire {threshold:.2?} (pinned budget {budget_ms:.1} \
         ms).\n  note: a >{GATE_MARGIN}x regression is a real performance regression, not \
         measurement noise\n  note: fix the regression, or re-pin the budget as a reviewed \
         decision — see benchmarks/gof-bench/BUDGETS.md for the procedure\n  help: re-measure \
         first to confirm: cargo bench -p gof-bench -- \"dev build\""
    );
}

#[test]
#[ignore = "timing gate — run explicitly in release: cargo test --release -p gof-bench --test dev_build_budget_gate -- --ignored --nocapture"]
fn uncached_dev_build_pipeline_stays_within_budget() {
    refuse_debug_profile("uncached");
    assert_within_budget("uncached", UNCACHED_PIPELINE_MS, dev_build_uncached_object);
}

#[test]
#[ignore = "timing gate — run explicitly in release: cargo test --release -p gof-bench --test dev_build_budget_gate -- --ignored --nocapture"]
fn cache_hit_dev_build_pipeline_stays_within_budget() {
    refuse_debug_profile("cache-hit");
    let cache = IrCache::new(dev_build_scratch_dir("gate-cache-hit"));
    dev_build_store_cache_entry(&cache);
    assert_within_budget("cache-hit", CACHE_HIT_PIPELINE_MS, || {
        dev_build_cache_hit_object(&cache)
    });
}
