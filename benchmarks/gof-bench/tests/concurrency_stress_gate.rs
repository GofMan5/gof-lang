//! The concurrency stress gate (audit P8) plus the wake-latency
//! tripwire (audit P7's latency claim, measured instead of
//! wall-clock-asserted in the default suite):
//!
//! - **10k-task stress smoke** — spawn 10,000 tasks, join all of them,
//!   assert completion with the exact expected sum. This is a
//!   COMPLETION gate, not a latency gate: the assert is that every
//!   spawned task round-trips (a task lost to a spawn failure, a lost
//!   join, or a dropped wake changes the sum), with a deliberately
//!   generous wall-clock ceiling that exists only as hang detection.
//! - **wake-latency tripwire** — in the dev_build_budget_gate mold:
//!   re-measures the same rendezvous wake fixture the benchmark
//!   measures and fails loudly when the median per-exchange wake
//!   round trip exceeds 3x the pinned budget — a margin that absorbs
//!   machine noise while still catching the 1000x-class regression
//!   (a polling floor replacing direct notification) that would
//!   destroy the claim.
//!
//! The numbers that matter live in the benchmark record
//! ([MEASUREMENTS.md](../MEASUREMENTS.md)).
//!
//! Opt-in on purpose: wall-clock-sensitive stress never runs in the
//! default test suite (BUDGETS.md's rule) — the tests are `#[ignore]d`
//! and must be invoked explicitly, in the RELEASE profile (the audit
//! measured the same stress fixture at 23.8 s under the debug
//! interpreter; release is an order of magnitude faster, and the gates
//! refuse debug runs with instructions):
//!
//! ```text
//! cargo test --release -p gof-bench --test concurrency_stress_gate -- --ignored --nocapture
//! ```
//!
//! The fixtures and the run helpers are the SAME lib helpers the
//! benchmark uses (`gof_bench::concurrency_runtime`), so the gates
//! exercise exactly what the benchmark measures — zero drift by
//! construction. Every expected value is computed from the same round
//! constant the benchmark's throughput uses, so a fixture edited out
//! of sync with its constant fails here instead of silently
//! mislabeling the measurement.

use gof_bench::concurrency_runtime::wake_budget::{GATE_MARGIN, RENDEZVOUS_WAKE_EXCHANGE_US};
use gof_bench::concurrency_runtime::{
    RENDEZVOUS_EXCHANGES, STRESS_TASKS, rendezvous_wake_expected, rendezvous_wake_module,
    run_rendezvous_wake, run_stress_10k, stress_10k_expected, stress_10k_module,
};
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Hang-detection ceiling, deliberately generous (~30x the measured
/// release number): a real failure mode here is starvation or a lost
/// wake hanging the join, not slowness — the completion assert is the
/// gate, the ceiling just turns a hang into a loud failure.
const HANG_CEILING: Duration = Duration::from_secs(120);

/// Discarded runs before measurement (allocator, code pages, thread
/// churn — not part of the budget).
const WARMUP_RUNS: usize = 2;

/// Measured runs; the MEDIAN (noise-robust) is compared against the
/// budget, the same shape as the dev-build budget gate.
const MEASURE_RUNS: usize = 11;

fn refuse_debug_profile(gate: &str) {
    if cfg!(debug_assertions) {
        panic!(
            "the {gate} gate must run in the RELEASE profile: the stress fixture spawns 10,000 \
             OS threads and the debug interpreter measured 23.8 s on the same fixture in the \
             Phase 3 audit.\n  help: cargo test --release -p gof-bench --test \
             concurrency_stress_gate -- --ignored --nocapture"
        );
    }
}

fn median(durations: &mut [Duration]) -> Duration {
    durations.sort();
    durations[durations.len() / 2]
}

/// Spawn 10,000 tasks, join them all, and assert every one of them
/// completed: the sum is exact, so a task lost anywhere in the
/// spawn/await path fails the gate.
#[test]
#[ignore = "stress gate — run explicitly in release: cargo test --release -p gof-bench --test concurrency_stress_gate -- --ignored --nocapture"]
fn ten_thousand_spawned_tasks_all_join_and_complete() {
    refuse_debug_profile("10k-stress");

    let module = stress_10k_module();
    let started = Instant::now();
    let outcome = black_box(run_stress_10k(&module));
    let elapsed = started.elapsed();

    eprintln!(
        "10k-stress: {STRESS_TASKS} spawn+join round trips in {:.2?} ({:.0} tasks/s, {:.3} ms per \
         round trip)",
        elapsed,
        STRESS_TASKS as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / STRESS_TASKS as f64,
    );

    assert_eq!(
        outcome,
        stress_10k_expected(),
        "the 10k-task stress program must complete with the exact expected sum — a task lost to a \
         spawn failure or a dropped join changes it"
    );
    assert!(
        elapsed <= HANG_CEILING,
        "the 10k-task stress program took {elapsed:.2?}, past the {HANG_CEILING:.0?} \
         hang-detection ceiling: this is starvation or a lost wake, not a slow run"
    );
}

/// The wake-latency tripwire (audit P7's "wakes fast, no polling
/// floor" claim, measured): re-measures the rendezvous wake fixture —
/// every exchange a blocked `recv` woken by a `send`, in both
/// directions — through the same lib helper the benchmark uses, and
/// fails loudly when the median per-exchange round trip exceeds the
/// pinned budget's regression margin. A polling floor lands at tens
/// of milliseconds per exchange; the tripwire sits three orders of
/// magnitude below that.
#[test]
#[ignore = "timing gate — run explicitly in release: cargo test --release -p gof-bench --test concurrency_stress_gate -- --ignored --nocapture"]
fn rendezvous_wake_latency_stays_within_budget() {
    refuse_debug_profile("wake-latency");

    let module = rendezvous_wake_module();

    // Completion first — also the fixture/constant zero-drift check.
    let first = run_rendezvous_wake(&module);
    assert_eq!(
        first,
        rendezvous_wake_expected(),
        "the rendezvous wake fixture must complete with the exact expected sum"
    );

    for _ in 0..WARMUP_RUNS {
        black_box(run_rendezvous_wake(&module));
    }
    let mut durations: Vec<Duration> = Vec::with_capacity(MEASURE_RUNS);
    for _ in 0..MEASURE_RUNS {
        let started = Instant::now();
        black_box(run_rendezvous_wake(&module));
        durations.push(started.elapsed());
    }

    let median_run = median(&mut durations);
    let per_exchange_us = median_run.as_secs_f64() * 1_000_000.0 / RENDEZVOUS_EXCHANGES as f64;
    let tripwire_us = RENDEZVOUS_WAKE_EXCHANGE_US * GATE_MARGIN;
    eprintln!(
        "wake-latency: median run {median_run:.2?} over {RENDEZVOUS_EXCHANGES} exchanges = \
         {per_exchange_us:.2} us per exchange (min {:.2?}, max {:.2?}) against a \
         {RENDEZVOUS_WAKE_EXCHANGE_US:.1} us budget — tripwire at {tripwire_us:.1} us",
        durations[0],
        durations[durations.len() - 1],
    );
    assert!(
        per_exchange_us <= tripwire_us,
        "the rendezvous wake latency regressed past the budget's {GATE_MARGIN}x regression \
         margin: {per_exchange_us:.2} us per exchange > tripwire {tripwire_us:.1} us (pinned \
         budget {RENDEZVOUS_WAKE_EXCHANGE_US:.1} us).\n  note: a >{GATE_MARGIN}x regression \
         here is a real wake-path regression — e.g. a polling floor replacing direct \
         notification — not measurement noise\n  note: fix the regression, or re-pin the \
         budget as a reviewed decision — see benchmarks/gof-bench/MEASUREMENTS.md for the \
         procedure\n  help: re-measure first to confirm: cargo bench -p gof-bench --bench \
         concurrency_runtime -- \"rendezvous wake\""
    );
}
