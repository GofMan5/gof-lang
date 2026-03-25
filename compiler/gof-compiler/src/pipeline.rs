use crate::ast::{Module, parse};
use crate::backend::{BackendArtifact, lower as lower_backend};
use crate::cst::CstModule;
use crate::diagnostics::Diagnostics;
use crate::formatter::format_module;
use crate::hir::{HirModule, lower as lower_hir};
use crate::interpreter::{Value, run as run_interpreter};
use crate::lexer::lex;
use crate::mir::{MirModule, lower as lower_mir};
use crate::source::SourceFile;
use crate::ssa::{SsaModule, lower as lower_ssa};
use crate::typed_hir::{TypedModule, lower as lower_typed};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub enum CompileMode {
    Library,
    Executable,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledModule {
    pub cst: CstModule,
    pub ast: Module,
    pub hir: HirModule,
    pub typed_hir: TypedModule,
    pub mir: MirModule,
    pub ssa: SsaModule,
    pub backend: BackendArtifact,
}

pub fn compile_source(
    source: &SourceFile,
    mode: CompileMode,
) -> Result<CompiledModule, Diagnostics> {
    let tokens = lex(source)?;
    let cst = CstModule::new(tokens);
    let ast = parse(&cst)?;
    let hir = lower_hir(&ast);
    let typed_hir = lower_typed(&hir)?;
    let mir = lower_mir(&typed_hir);
    let ssa = lower_ssa(&mir);
    let backend = lower_backend(
        &ssa,
        match mode {
            CompileMode::Library => "library",
            CompileMode::Executable => "executable",
        },
    );

    Ok(CompiledModule {
        cst,
        ast,
        hir,
        typed_hir,
        mir,
        ssa,
        backend,
    })
}

pub fn format_source(source: &SourceFile) -> Result<String, Diagnostics> {
    let compiled = compile_source(source, CompileMode::Library)?;
    Ok(format_module(&compiled.ast))
}

pub fn run_module(source: &SourceFile) -> Result<Value, Diagnostics> {
    let compiled = compile_source(source, CompileMode::Executable)?;
    run_interpreter(&compiled.ast)
}

#[cfg(test)]
mod tests {
    use super::{CompileMode, compile_source};
    use crate::source::SourceFile;

    #[test]
    fn pipeline_emits_backend_artifact() {
        let source = SourceFile::new("hello.gof", "fn main():\n    return 42\n");
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");
        assert_eq!(compiled.backend.backend, "bootstrap-ir-v0");
        assert_eq!(compiled.ssa.functions.len(), 1);
    }

    #[test]
    fn pipeline_supports_locals_and_calls() {
        let source = SourceFile::new(
            "hello.gof",
            "fn add(a, b):\n    return a + b\nfn main():\n    mut total = add(40, 1)\n    total = total + 1\n    return total\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");
        assert_eq!(compiled.ssa.functions.len(), 2);
    }
}
