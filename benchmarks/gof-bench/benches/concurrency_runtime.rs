//! The concurrency runtime measurement surface (Phase 3, audit P8):
//! end-to-end gof programs exercising the task/channel/cancellation
//! primitives, measured through the public interpreter seam
//! (`run_with_output`), exactly like the pre-existing rendezvous
//! select bench below. The fixture programs, the round counts, and
//! the compile/run helpers live in the crate lib
//! (`gof_bench::concurrency_runtime`) so the gates
//! (`tests/concurrency_stress_gate.rs`) enforce exactly what these
//! benches measure — zero drift by construction.
//!
//! What each bench measures (the full record, with machine identity
//! and per-number interpretation, lives in
//! [MEASUREMENTS.md](../MEASUREMENTS.md)):
//!
//! - **blocking rendezvous select** (pre-existing) — one end-to-end
//!   run of the 2-task rendezvous select program, module setup
//!   included; the baseline concurrency-end-to-end shape.
//! - **spawn join round trips** — 128 sequential `go trivial()` +
//!   `await` round trips per run (rendezvous-free trivial task);
//!   throughput is round trips per second, so the per-round-trip
//!   spawn/join latency is its reciprocal.
//! - **channel send recv pairs** — 1024 buffered send/recv pairs per
//!   run (capacity 64, sender and receiver on the same thread);
//!   throughput is pairs per second.
//! - **rendezvous wake round trips** — 512 ping/pong exchanges over a
//!   pair of rendezvous channels per run; every exchange is a blocked
//!   `recv` woken by a `send` (in both directions), so the
//!   per-exchange wake latency is the reciprocal of the reported
//!   throughput. This is the wake-latency distribution the
//!   interpreter's old wall-clock cancellation tests asserted
//!   informally (audit P7): criterion's per-iteration samples are the
//!   distribution, and a polling-floor regression shows up here as
//!   orders of magnitude, not as a flaky 50 ms assert.
//! - **cancel wake round trips** — 128 rounds per run, each: spawn a
//!   waiter blocked on `recv` with a cancel token, handshake on a
//!   `ready` channel, `cancel`, `join`. The cancel-side wake path the
//!   interpreter's default-suite cancellation tests assert, measured
//!   end-to-end per round (spawn and join included — comparable to
//!   the spawn/join bench, which is the same structure minus the
//!   blocking and the cancel). This bench carries a WATCHDOG: the
//!   interpreter currently has an intermittent lost-wake on this
//!   exact path (the finding in MEASUREMENTS.md), and the watchdog
//!   turns a would-be silent hang of `cargo bench` into a loud
//!   fast failure.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gof_bench::concurrency_runtime::{
    CANCEL_WAKE_ROUNDS, CHANNEL_PAIRS, RENDEZVOUS_EXCHANGES, SPAWN_JOIN_ROUNDS, cancel_wake_module,
    channel_pairs_module, rendezvous_wake_module, run_cancel_wake, run_channel_pairs,
    run_rendezvous_wake, run_spawn_join, spawn_join_module,
};
use gof_compiler::interpreter::run_with_output;
use gof_compiler::{CompileMode, SourceFile, compile_source};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn benchmark_blocking_rendezvous_select(c: &mut Criterion) {
    let source = SourceFile::new(
        "bench-select-rendezvous.gof",
        "fn sender(ch: channel[int]) -> Result[unit, RuntimeError]:\n    select:\n        sent = send(ch, 7):\n            return sent\n\nfn main() -> Result[int, RuntimeError]:\n    ch: channel[int] = channel(0)\n    sender_task = go sender(ch)\n    select:\n        received = recv(ch):\n            send_outcome = await sender_task\n            match send_outcome:\n                Result.Ok(_):\n                    return Result.Ok(received?)\n                Result.Err(error):\n                    return Result.Err(error)\n",
    );
    let compiled = compile_source(&source, CompileMode::Executable)
        .expect("concurrency benchmark source should compile");

    c.bench_function("runtime blocking rendezvous select", |bench| {
        bench.iter(|| {
            let result = run_with_output(&compiled.ast)
                .expect("blocking rendezvous select benchmark should run");
            black_box(result.value.cli_text());
            black_box(result.stdout);
        });
    });
}

/// Spawn/join round-trip latency: `SPAWN_JOIN_ROUNDS` sequential
/// go+await round trips of a rendezvous-free trivial task per
/// iteration; reported as round trips per second.
fn benchmark_spawn_join_round_trips(c: &mut Criterion) {
    let module = spawn_join_module();

    let mut group = c.benchmark_group("concurrency");
    group.throughput(Throughput::Elements(SPAWN_JOIN_ROUNDS));
    group.bench_function("spawn join round trips", |bench| {
        bench.iter(|| black_box(run_spawn_join(&module)));
    });
    group.finish();
}

/// Channel throughput: `CHANNEL_PAIRS` buffered send/recv pairs per
/// iteration; reported as pairs per second.
fn benchmark_channel_send_recv_pairs(c: &mut Criterion) {
    let module = channel_pairs_module();

    let mut group = c.benchmark_group("concurrency");
    group.throughput(Throughput::Elements(CHANNEL_PAIRS));
    group.bench_function("channel send recv pairs", |bench| {
        bench.iter(|| black_box(run_channel_pairs(&module)));
    });
    group.finish();
}

/// Wake latency, send side: `RENDEZVOUS_EXCHANGES` ping/pong
/// exchanges per iteration, each a blocked recv woken by a send (both
/// directions); reported as exchanges per second, so the per-exchange
/// wake round-trip latency is the reciprocal.
fn benchmark_rendezvous_wake_round_trips(c: &mut Criterion) {
    let module = rendezvous_wake_module();

    let mut group = c.benchmark_group("concurrency");
    group.throughput(Throughput::Elements(RENDEZVOUS_EXCHANGES));
    group.bench_function("rendezvous wake round trips", |bench| {
        bench.iter(|| black_box(run_rendezvous_wake(&module)));
    });
    group.finish();
}

/// A no-progress watchdog for the cancel-wake bench: the interpreter
/// currently has an intermittent lost-wake on this fixture's exact
/// cancel path (the finding in [MEASUREMENTS.md](../MEASUREMENTS.md)
/// — a blocked operation that should be woken by `cancel` never
/// returns). Without the watchdog that manifests as `cargo bench`
/// freezing with no output; with it, the hang becomes a loud failure
/// within `TIMEOUT` instead. The margin is ~3000x the measured
/// per-iteration time, so a merely slow machine never trips it.
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(60);
const WATCHDOG_POLL: Duration = Duration::from_secs(5);

fn watchdog(report: &'static str) -> (Arc<AtomicU64>, Arc<AtomicBool>) {
    let heartbeat = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let watch_heartbeat = Arc::clone(&heartbeat);
    let watch_done = Arc::clone(&done);
    std::thread::spawn(move || {
        loop {
            if watch_done.load(Ordering::SeqCst) {
                return;
            }
            let last = watch_heartbeat.load(Ordering::SeqCst);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since_epoch| since_epoch.as_millis() as u64)
                .unwrap_or_default();
            if last != 0 && now.saturating_sub(last) > WATCHDOG_TIMEOUT.as_millis() as u64 {
                eprintln!(
                    "{report}: NO PROGRESS for over {WATCHDOG_TIMEOUT:?} — this is the intermittent \
                 lost-wake hang documented in benchmarks/gof-bench/MEASUREMENTS.md, not a slow \
                 run; exiting loudly instead of hanging"
                );
                std::process::exit(2);
            }
            std::thread::sleep(WATCHDOG_POLL);
        }
    });
    (heartbeat, done)
}

/// Wake latency, cancel side: `CANCEL_WAKE_ROUNDS` rounds per
/// iteration, each a blocked recv woken by a token cancel plus its
/// spawn and join; reported as rounds per second (the spawn/join
/// bench is the same structure without the blocking and cancel).
fn benchmark_cancel_wake_round_trips(c: &mut Criterion) {
    let module = cancel_wake_module();
    let (heartbeat, done) = watchdog("concurrency/cancel wake round trips");

    let mut group = c.benchmark_group("concurrency");
    group.throughput(Throughput::Elements(CANCEL_WAKE_ROUNDS));
    group.bench_function("cancel wake round trips", |bench| {
        bench.iter(|| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since_epoch| since_epoch.as_millis() as u64)
                .unwrap_or_default();
            heartbeat.store(now, Ordering::SeqCst);
            black_box(run_cancel_wake(&module))
        });
    });
    group.finish();
    done.store(true, Ordering::SeqCst);
}

criterion_group!(
    benches,
    benchmark_blocking_rendezvous_select,
    benchmark_spawn_join_round_trips,
    benchmark_channel_send_recv_pairs,
    benchmark_rendezvous_wake_round_trips,
    benchmark_cancel_wake_round_trips,
);
criterion_main!(benches);
