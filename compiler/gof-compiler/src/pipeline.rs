use crate::ast::Module;
use crate::backend::{BackendArtifact, lower as lower_backend};
use crate::cst::CstModule;
use crate::diagnostics::Diagnostics;
use crate::formatter::format_module;
use crate::hir::{HirModule, lower as lower_hir};
use crate::interpreter::{Value, run as run_interpreter};
use crate::mir::{MirModule, lower as lower_mir};
use crate::module_graph::{load_module_graph, parse_single_source};
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
    let ast = load_module_graph(source)?;
    let cst = CstModule::new(
        crate::lexer::lex(source)
            .map_err(|diagnostics| diagnostics.with_source_path(source.path()))?,
    );
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
    let module = parse_single_source(source)?;
    Ok(format_module(&module))
}

pub fn run_module(source: &SourceFile) -> Result<Value, Diagnostics> {
    let compiled = compile_source(source, CompileMode::Executable)?;
    run_interpreter(&compiled.ast)
}

#[cfg(test)]
mod tests {
    use super::{CompileMode, compile_source};
    use crate::source::SourceFile;
    use crate::ssa::SsaInstruction;
    use crate::typed_hir::Type;

    #[test]
    fn pipeline_emits_backend_artifact() {
        let source = SourceFile::new("hello.gof", "fn main():\n    return 42\n");
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");
        assert_eq!(compiled.backend.backend, "bootstrap-ir-v0");
        assert_eq!(compiled.ssa.functions.len(), 1);
        assert!(compiled.ast.imports.is_empty());
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

    #[test]
    fn pipeline_emits_spawn_and_await_for_tasks() {
        let source = SourceFile::new(
            "tasks.gof",
            "fn lucky() -> int:\n    return 12\nfn main() -> int:\n    task = go lucky()\n    return await task\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");
        let instructions = compiled.ssa.functions[1]
            .values
            .iter()
            .map(|value| &value.instruction)
            .collect::<Vec<_>>();

        assert!(
            instructions
                .iter()
                .any(|instruction| matches!(instruction, SsaInstruction::Spawn { .. }))
        );
        assert!(
            instructions
                .iter()
                .any(|instruction| matches!(instruction, SsaInstruction::Await { .. }))
        );
        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        assert_eq!(compiled.typed_hir.functions[1].return_type, Type::Int);
        match &compiled.typed_hir.functions[1].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Task(Box::new(Type::Int)));
            }
            other => panic!("expected bind statement, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_preserves_explicit_return_contracts() {
        let source = SourceFile::new(
            "typed.gof",
            "fn relay() -> bool:\n    return true\nfn main() -> bool:\n    return relay()\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(
            compiled.ast.functions[0].return_type.as_ref().unwrap().name,
            "bool"
        );
        assert_eq!(
            compiled.hir.functions[0].return_type.as_ref().unwrap().name,
            "bool"
        );
        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Bool);
        assert_eq!(compiled.typed_hir.functions[1].return_type, Type::Bool);
    }

    #[test]
    fn pipeline_supports_lists_and_indexing() {
        let source = SourceFile::new(
            "lists.gof",
            "fn main() -> int:\n    values: list = [3, 5, 8]\n    return values[1] + len(values)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected list bind statement, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BuildList(_)))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Index { .. }))
        );
    }

    #[test]
    fn pipeline_resolves_local_imports() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("math.gof");
        let main_path = temp.path().join("main.gof");

        std::fs::write(
            &helper_path,
            "fn square(x: int) -> int:\n    return x * x\n",
        )
        .expect("helper module should be written");
        std::fs::write(
            &main_path,
            "import math\n\nfn main() -> int:\n    return square(7)\n",
        )
        .expect("main module should be written");

        let source = SourceFile::from_path(&main_path).expect("source should load");
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.ast.imports.len(), 1);
        assert_eq!(compiled.ssa.functions.len(), 2);
        assert_eq!(compiled.typed_hir.functions[0].name, "square");
        assert_eq!(compiled.typed_hir.functions[1].name, "main");
    }

    #[test]
    fn pipeline_supports_structs_and_fields() {
        let source = SourceFile::new(
            "structs.gof",
            "struct Point:\n    x: int\n    y: int\n\nfn main() -> int:\n    point: Point = Point(3, 5)\n    return point.x + point.y\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.ast.structs.len(), 1);
        assert_eq!(compiled.hir.structs.len(), 1);
        assert_eq!(compiled.typed_hir.structs[0].name, "Point");
        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BuildStruct { .. }))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::LoadField { .. }))
        );
    }

    #[test]
    fn pipeline_supports_logical_operators() {
        let source = SourceFile::new(
            "logic.gof",
            "fn main() -> bool:\n    return not false and true or false\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Bool);
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Unary { .. }))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::Binary {
                        op: crate::ast::BinaryOp::And | crate::ast::BinaryOp::Or,
                        ..
                    }
                ))
        );
    }
}
