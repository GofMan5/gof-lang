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
pub use interpreter::ExecutionResult;
pub use pipeline::{
    CompileMode, CompiledModule, compile_source, format_source, run_module, run_module_with_output,
};
pub use source::{SourceFile, Span};
