use criterion::{Criterion, criterion_group, criterion_main};
use gof_compiler::interpreter::run_with_output;
use gof_compiler::{CompileMode, SourceFile, compile_source};
use std::hint::black_box;

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

criterion_group!(benches, benchmark_blocking_rendezvous_select);
criterion_main!(benches);