pub mod ast;
pub mod backend;
pub mod cst;
pub mod diagnostics;
pub mod formatter;
pub mod hir;
pub mod interpreter;
pub mod lexer;
pub mod mir;
pub mod module_graph;
pub mod package;
pub mod pipeline;
pub mod source;
pub mod ssa;
pub mod token;
pub mod typed_hir;

pub use diagnostics::{Diagnostic, Diagnostics, Severity};
pub use interpreter::{
    ExecutionResult, TestExecutionOutcome, TestExecutionResult, TestModuleState,
    TestRuntimeOptions, run_test_function_with_output,
};
pub use pipeline::{
    CompileMode, CompiledModule, build_embedded_source_bundle, compile_source,
    compile_source_with_provider, format_source, run_embedded_bundle_with_output,
    run_embedded_bundle_with_output_and_args, run_module, run_module_with_output,
    run_module_with_output_and_args,
};
pub use source::{
    EmbeddedPackageContext, EmbeddedSourceBundle, EmbeddedSourceProvider, FileSystemSourceProvider,
    SourceFile, SourceProvider, Span, normalize_source_path,
};
