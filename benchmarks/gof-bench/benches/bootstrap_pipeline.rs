use criterion::{Criterion, criterion_group, criterion_main};
use gof_compiler::{CompileMode, SourceFile, compile_source};
use std::hint::black_box;

fn benchmark_compile(c: &mut Criterion) {
    c.bench_function("compile bootstrap hello", |bench| {
        bench.iter(|| {
            let source = SourceFile::new("bench.gof", "fn main():\n    return 40 + 2\n");
            let compiled = compile_source(&source, CompileMode::Executable)
                .expect("benchmark source should compile");
            black_box(compiled.backend.backend);
        });
    });
}

criterion_group!(benches, benchmark_compile);
criterion_main!(benches);
