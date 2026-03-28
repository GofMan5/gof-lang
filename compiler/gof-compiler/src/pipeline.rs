use crate::ast::Module;
use crate::backend::{BackendArtifact, lower as lower_backend};
use crate::cst::CstModule;
use crate::diagnostics::Diagnostics;
use crate::formatter::format_module;
use crate::hir::{HirModule, lower as lower_hir};
use crate::interpreter::{ExecutionResult, Value, run_with_output as run_interpreter_with_output};
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
    Ok(run_module_with_output(source)?.value)
}

pub fn run_module_with_output(source: &SourceFile) -> Result<ExecutionResult, Diagnostics> {
    let compiled = compile_source(source, CompileMode::Executable)?;
    run_interpreter_with_output(&compiled.ast)
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
    fn pipeline_supports_await_result_builtin() {
        let source = SourceFile::new(
            "await_result.gof",
            "fn lucky() -> int:\n    return 7\nfn main() -> Result[int, RuntimeError]:\n    task = go lucky()\n    return await_result(task)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(
            compiled.typed_hir.functions[1].return_type,
            Type::Result(
                Box::new(Type::Int),
                Box::new(Type::Enum("RuntimeError".to_string())),
            )
        );
        match &compiled.typed_hir.functions[1].body[1] {
            crate::typed_hir::TypedStmt::Return(expr) => {
                assert_eq!(
                    expr.ty,
                    Type::Result(
                        Box::new(Type::Int),
                        Box::new(Type::Enum("RuntimeError".to_string())),
                    )
                );
            }
            other => panic!("expected return statement, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_supports_cancellable_await_result_builtin() {
        let source = SourceFile::new(
            "await_result_token.gof",
            "fn lucky() -> int:\n    return 7\nfn main() -> Result[int, RuntimeError]:\n    task = go lucky()\n    token = cancel_token()\n    return await_result(task, token)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[1].body[2] {
            crate::typed_hir::TypedStmt::Return(expr) => {
                assert_eq!(
                    expr.ty,
                    Type::Result(
                        Box::new(Type::Int),
                        Box::new(Type::Enum("RuntimeError".to_string())),
                    )
                );
            }
            other => panic!("expected return statement, got {other:?}"),
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
    fn pipeline_supports_parameterized_builtin_type_annotations() {
        let source = SourceFile::new(
            "typed_params.gof",
            "fn first(values: list[int]) -> int:\n    return values[0]\nfn main() -> Result[dict[int], RuntimeError]:\n    ch: channel[int] = channel()\n    send(ch, first([7, 9]))?\n    return Result.Ok({\"ok\": recv(ch)?})\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(
            compiled.typed_hir.functions[0].params[0].ty,
            Type::List(Box::new(Type::Int))
        );
        assert_eq!(
            compiled.typed_hir.functions[1].return_type,
            Type::Result(
                Box::new(Type::Dict(Box::new(Type::Int))),
                Box::new(Type::Enum("RuntimeError".to_string()))
            )
        );
        assert!(matches!(
            &compiled.typed_hir.functions[1].body[1],
            crate::typed_hir::TypedStmt::Expr(_)
        ));
    }

    #[test]
    fn pipeline_supports_unary_minus_division_and_modulo() {
        let source = SourceFile::new(
            "numeric.gof",
            "fn main() -> int:\n    base = -6 / 3\n    return base % 4\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::Unary {
                        op: crate::ast::UnaryOp::Neg,
                        ..
                    }
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::Binary {
                        op: crate::ast::BinaryOp::Div | crate::ast::BinaryOp::Mod,
                        ..
                    }
                ))
        );
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
    fn pipeline_supports_dict_literals() {
        let source = SourceFile::new(
            "dicts.gof",
            "fn main() -> int:\n    values: dict = {\"ok\": 2, \"warn\": 3}\n    return values[\"ok\"] + len(values)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Dict(Box::new(Type::Int)));
            }
            other => panic!("expected dict bind statement, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BuildDict(_)))
        );
    }

    #[test]
    fn pipeline_supports_dict_view_builtins() {
        let source = SourceFile::new(
            "dict_views.gof",
            "fn main() -> int:\n    metrics: dict = {\"critical\": 5, \"ok\": 7, \"warn\": 2}\n    names = keys(metrics)\n    counts = values(metrics)\n    return len(names) + counts[0]\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::String)));
            }
            other => panic!("expected keys bind statement, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[2] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected values bind statement, got {other:?}"),
        }
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
    fn pipeline_resolves_local_package_dependencies() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let math_source_root = math_root.join("src");
        let app_root = temp.path().join("package_app");
        let app_source_root = app_root.join("src");
        let main_path = app_source_root.join("main.gof");

        std::fs::create_dir_all(&math_source_root).expect("math source root should exist");
        std::fs::create_dir_all(&app_source_root).expect("app source root should exist");
        std::fs::write(
            math_root.join("gof.mod"),
            "module = \"example/package_math\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("math manifest should be written");
        std::fs::write(
            math_source_root.join("lib.gof"),
            "import ops\n\nfn square(value: int) -> int:\n    return multiply(value, value)\n",
        )
        .expect("math lib should be written");
        std::fs::write(
            math_source_root.join("ops.gof"),
            "fn multiply(lhs: int, rhs: int) -> int:\n    return lhs * rhs\n",
        )
        .expect("math ops should be written");
        std::fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
        )
        .expect("app manifest should be written");
        std::fs::write(
            &main_path,
            "import package_math\n\nfn main() -> int:\n    return square(9) + square(3)\n",
        )
        .expect("app main should be written");

        let source = SourceFile::from_path(&main_path).expect("source should load");
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.ast.functions.len(), 3);
        assert_eq!(compiled.typed_hir.functions.len(), 3);
        assert_eq!(compiled.typed_hir.functions[2].name, "main");
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

    #[test]
    fn pipeline_supports_enums_and_variant_constants() {
        let source = SourceFile::new(
            "enums.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> bool:\n    return Status.Ready != Status.Busy\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.ast.enums.len(), 1);
        assert_eq!(compiled.hir.enums.len(), 1);
        assert_eq!(compiled.typed_hir.enums[0].name, "Status");
        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Bool);
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::ConstEnumVariant { .. }))
        );
    }

    #[test]
    fn pipeline_supports_exhaustive_match_over_enums() {
        let source = SourceFile::new(
            "match.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 10\n        Status.Busy:\n            return 20\n\nfn main() -> int:\n    return score(Status.Busy)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].return_type, Type::Int);
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BeginMatch { .. }))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::MatchArm { .. }))
        );
    }

    #[test]
    fn pipeline_supports_payload_enums_and_destructuring_match() {
        let source = SourceFile::new(
            "payload_match.gof",
            "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn score(state: JobState) -> int:\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n\nfn main() -> int:\n    state: JobState = JobState.Running(42)\n    return score(state)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.enums[0].variants[1].name, "Running");
        assert_eq!(compiled.typed_hir.enums[0].variants[1].fields.len(), 1);
        assert!(compiled.ssa.functions[1].values.iter().any(|value| {
            matches!(
                &value.instruction,
                SsaInstruction::ConstEnumVariant {
                    enum_name,
                    variant,
                    args,
                } if enum_name == "JobState" && variant == "Running" && args.len() == 1
            )
        }));
        assert!(compiled.ssa.functions[0].values.iter().any(|value| {
            matches!(
                &value.instruction,
                SsaInstruction::MatchArm {
                    enum_name,
                    variant,
                    bindings,
                } if enum_name == "JobState" && variant == "Running" && bindings == &vec!["pid".to_string()]
            )
        }));
    }

    #[test]
    fn pipeline_supports_explicit_comparison_semantics() {
        let source = SourceFile::new(
            "comparison_surface.gof",
            "struct Snapshot:\n    count: int\n    label: string\n\nenum Stage:\n    Draft\n    Published(version: int)\n\nfn main() -> Result[int, RuntimeError]:\n    left = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    right = json_parse(\"{\\\"count\\\": 2, \\\"label\\\": \\\"beta\\\"}\")?\n    snapshot_a: Snapshot = Snapshot(2, \"beta\")\n    snapshot_b: Snapshot = Snapshot(2, \"beta\")\n    stage_a: Stage = Stage.Published(3)\n    stage_b: Stage = Stage.Published(3)\n    ok_a: Result[int, RuntimeError] = Result.Ok(7)\n    ok_b: Result[int, RuntimeError] = Result.Ok(7)\n    assert(\"alpha\" < \"beta\", \"expected lexicographic string ordering\")\n    assert(left == right, \"expected structural json equality\")\n    assert(snapshot_a == snapshot_b, \"expected structural struct equality\")\n    assert(stage_a == stage_b, \"expected payload enum equality\")\n    assert(ok_a == ok_b, \"expected result equality\")\n    assert(sleep(0) == sleep(0), \"expected unit equality\")\n    assert([1, 2] == [1, 2], \"expected list equality\")\n    assert({\"ok\": 2} == {\"ok\": 2}, \"expected dict equality\")\n    return Result.Ok(42)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(
            compiled.typed_hir.functions[0].return_type,
            Type::Result(
                Box::new(Type::Int),
                Box::new(Type::Enum("RuntimeError".to_string()))
            )
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::Binary {
                        op: crate::ast::BinaryOp::Lt,
                        ..
                    }
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::Binary {
                        op: crate::ast::BinaryOp::Eq,
                        ..
                    }
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Propagate { .. }))
        );
    }

    #[test]
    fn pipeline_supports_receiver_methods() {
        let source = SourceFile::new(
            "methods.gof",
            "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(compiled.typed_hir.functions[0].symbol_name, "Point.total");
        assert!(
            compiled.ssa.functions[1]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "Point.total"
                ))
        );
    }

    #[test]
    fn pipeline_supports_print_builtin() {
        let source = SourceFile::new(
            "print.gof",
            "fn main() -> int:\n    print(\"gof\")\n    return 1\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert!(matches!(
            &compiled.typed_hir.functions[0].body[0],
            crate::typed_hir::TypedStmt::Expr(expr)
                if matches!(&expr.kind, crate::typed_hir::TypedExprKind::Call { callee, .. } if callee == "print")
                    && expr.ty == Type::Unit
        ));
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "print"
                ))
        );
    }

    #[test]
    fn pipeline_supports_append_and_contains_builtins() {
        let source = SourceFile::new(
            "helpers.gof",
            "fn main() -> bool:\n    values = append([1, 2], 3)\n    return contains(values, 3) and contains(\"gof-lang\", \"lang\")\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected helper bind, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "append"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "contains"
                ))
        );
    }

    #[test]
    fn pipeline_supports_string_helper_builtins() {
        let source = SourceFile::new(
            "text_helpers.gof",
            "fn main() -> int:\n    line = trim(\"  gof,lang  \")\n    parts = split(line, \",\")\n    merged = join(parts, \"-\")\n    if starts_with(merged, \"gof\") and ends_with(merged, \"lang\"):\n        return len(merged) + len(parts)\n    return 0\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::String)));
            }
            other => panic!("expected split bind, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "split"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "join"
                ))
        );
    }

    #[test]
    fn pipeline_supports_conversion_builtins() {
        let source = SourceFile::new(
            "conversion_helpers.gof",
            "fn main() -> Result[int, RuntimeError]:\n    raw = parse_int(trim(\" 41 \"))\n    parsed = raw?\n    rendered = \"gof-\" + to_string(parsed + 1)\n    assert(rendered == \"gof-42\", \"expected converted text\")\n    return Result.Ok(parsed + len(rendered))\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(
                    value.ty,
                    Type::Result(
                        Box::new(Type::Int),
                        Box::new(Type::Enum("RuntimeError".to_string()))
                    )
                );
            }
            other => panic!("expected parse_int bind, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::Int);
            }
            other => panic!("expected propagated parse_int bind, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[2] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::String);
            }
            other => panic!("expected to_string bind, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "parse_int"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Propagate { .. }))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                        &value.instruction,
                        SsaInstruction::Call { callee, .. } if callee == "to_string"
                ))
        );
    }

    #[test]
    fn pipeline_supports_sequence_helper_builtins() {
        let source = SourceFile::new(
            "sequence_helpers.gof",
            "fn main() -> Result[int, RuntimeError]:\n    values = [7, 1, 5, 3]\n    head = first(values)?\n    tail = last(values)?\n    middle = slice(values, 1, 3)?\n    reversed = reverse(values)\n    ordered = sort(values)\n    smallest = min(values)?\n    loudest = max([\"warn\", \"critical\", \"ok\"])?\n    return Result.Ok(head + tail + len(middle) + len(reversed) + len(ordered) + smallest + len(loudest))\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[3] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected slice bind, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "first"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "slice"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "sort"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "min"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "max"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Propagate { .. }))
        );
    }

    #[test]
    fn pipeline_supports_sleep_and_http_post_builtins() {
        let source = SourceFile::new(
            "bot_ops.gof",
            "fn main() -> Result[string, RuntimeError]:\n    sleep(0)\n    return http_post(\"https://example.invalid/send\", \"{}\", \"application/json\")\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Expr(expr) => {
                assert_eq!(expr.ty, Type::Unit);
                assert!(matches!(
                    &expr.kind,
                    crate::typed_hir::TypedExprKind::Call { callee, .. } if callee == "sleep"
                ));
            }
            other => panic!("expected sleep expression, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Return(expr) => {
                assert!(matches!(
                    &expr.kind,
                    crate::typed_hir::TypedExprKind::Call { callee, .. } if callee == "http_post"
                ));
                assert!(matches!(expr.ty, Type::Result(_, _)));
            }
            other => panic!("expected http_post return, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "sleep"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "http_post"
                ))
        );
    }

    #[test]
    fn pipeline_supports_timeout_token_and_cancel_after_builtins() {
        let source = SourceFile::new(
            "timeouts.gof",
            "fn main() -> bool:\n    token = timeout_token(25)\n    cancel_after(token, 0)\n    return is_cancelled(token)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::CancelToken);
                assert!(matches!(
                    &value.kind,
                    crate::typed_hir::TypedExprKind::Call { callee, .. } if callee == "timeout_token"
                ));
            }
            other => panic!("expected timeout_token bind, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Expr(expr) => {
                assert_eq!(expr.ty, Type::Unit);
                assert!(matches!(
                    &expr.kind,
                    crate::typed_hir::TypedExprKind::Call { callee, .. } if callee == "cancel_after"
                ));
            }
            other => panic!("expected cancel_after expression, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_supports_line_file_builtins() {
        let source = SourceFile::new(
            "line_io.gof",
            "fn main() -> Result[int, RuntimeError]:\n    write_lines(\"out.txt\", [\"alpha\", \"beta\"])?\n    lines = read_lines(\"out.txt\")?\n    return Result.Ok(len(join(lines, \"-\")))\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::String)));
            }
            other => panic!("expected read_lines bind, got {other:?}"),
        }

        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "write_lines"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "read_lines"
                ))
        );
    }

    #[test]
    fn pipeline_supports_csv_builtins() {
        let source = SourceFile::new(
            "csv_inventory.gof",
            "fn main() -> Result[int, RuntimeError]:\n    rows = csv_parse(\"name,count\\nalpha,2\\nbeta,5\")?\n    rendered = csv_stringify(rows)?\n    return Result.Ok(len(rows) + len(rendered))\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(
                    value.ty,
                    Type::List(Box::new(Type::List(Box::new(Type::String))))
                );
            }
            other => panic!("expected csv_parse bind, got {other:?}"),
        }
        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::String);
            }
            other => panic!("expected csv_stringify bind, got {other:?}"),
        }

        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "csv_parse"
                ))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "csv_stringify"
                ))
        );
    }

    #[test]
    fn pipeline_supports_range_builtin() {
        let source = SourceFile::new(
            "range_helpers.gof",
            "fn main() -> int:\n    values = range(1, 7, 2)\n    mut total = 0\n    for value in values:\n        total = total + value\n    return total\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Bind { value, .. } => {
                assert_eq!(value.ty, Type::List(Box::new(Type::Int)));
            }
            other => panic!("expected range bind, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    &value.instruction,
                    SsaInstruction::Call { callee, .. } if callee == "range"
                ))
        );
    }

    #[test]
    fn pipeline_supports_select_and_channel_builtins() {
        let source = SourceFile::new(
            "select.gof",
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel()\n    send(ch, 7)?\n    select:\n        received = recv(ch):\n            match received:\n                Result.Ok(value):\n                    return Result.Ok(value + 1)\n                Result.Err(error):\n                    return Result.Err(error)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert!(matches!(
            &compiled.typed_hir.functions[0].body[2],
            crate::typed_hir::TypedStmt::Select { arms } if arms.len() == 1
        ));
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BeginSelect))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::SelectArm { .. }))
        );
    }

    #[test]
    fn pipeline_supports_select_send_arms() {
        let source = SourceFile::new(
            "select_send.gof",
            "fn main() -> Result[int, RuntimeError]:\n    ch: channel = channel(1)\n    select:\n        sent = send(ch, 7):\n            sent?\n            return Result.Ok(recv(ch)?)\n        default:\n            return Result.Ok(0)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[1] {
            crate::typed_hir::TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(
                    arms[0].kind,
                    crate::typed_hir::TypedSelectArmKind::Send { .. }
                ));
                assert!(matches!(
                    arms[1].kind,
                    crate::typed_hir::TypedSelectArmKind::Default
                ));
            }
            other => panic!("expected select statement, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::SelectArm { .. }))
        );
    }

    #[test]
    fn pipeline_supports_explicit_channel_capacity_baseline() {
        let source = SourceFile::new(
            "channel_capacity.gof",
            "fn main() -> channel[int]:\n    return channel(0)\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert_eq!(
            compiled.typed_hir.functions[0].return_type,
            Type::Channel(Box::new(Type::Int))
        );
        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Return(expr) => {
                assert!(matches!(
                    &expr.kind,
                    crate::typed_hir::TypedExprKind::Call { callee, args }
                        if callee == "channel" && args.len() == 1
                ));
            }
            other => panic!("expected channel return, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_supports_select_default_arm() {
        let source = SourceFile::new(
            "select_default.gof",
            "fn main() -> int:\n    select:\n        default:\n            return 1\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        match &compiled.typed_hir.functions[0].body[0] {
            crate::typed_hir::TypedStmt::Select { arms } => {
                assert_eq!(arms.len(), 1);
                assert!(matches!(
                    arms[0].kind,
                    crate::typed_hir::TypedSelectArmKind::Default
                ));
            }
            other => panic!("expected select statement, got {other:?}"),
        }
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(
                    value.instruction,
                    SsaInstruction::SelectArm {
                        is_default: true,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn pipeline_rejects_negative_channel_capacity() {
        let source = SourceFile::new(
            "negative_channel_capacity.gof",
            "fn main() -> channel[int]:\n    return channel(-1)\n",
        );
        let diagnostics = compile_source(&source, CompileMode::Executable)
            .expect_err("negative channel capacity should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3097"]);
    }

    #[test]
    fn pipeline_supports_for_in_loops() {
        let source = SourceFile::new(
            "for.gof",
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return total\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert!(matches!(
            &compiled.typed_hir.functions[0].body[1],
            crate::typed_hir::TypedStmt::For { binding, .. } if binding == "value"
        ));
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::BeginFor { .. }))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::EndFor))
        );
    }

    #[test]
    fn pipeline_supports_break_and_continue() {
        let source = SourceFile::new(
            "loop_control.gof",
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3, 4]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Break))
        );
        assert!(
            compiled.ssa.functions[0]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Continue))
        );
    }

    #[test]
    fn pipeline_supports_result_annotations_and_propagation() {
        let source = SourceFile::new(
            "result_flow.gof",
            "enum MathError:\n    TooSmall\n    NotEven(value: int)\n\nfn halve(value: int) -> Result[int, MathError]:\n    if value < 2:\n        return Result.Err(MathError.TooSmall)\n    if value % 2 != 0:\n        return Result.Err(MathError.NotEven(value))\n    return Result.Ok(value / 2)\n\nfn compute() -> Result[int, MathError]:\n    half = halve(84)?\n    return Result.Ok(half)\n\nfn main() -> int:\n    outcome: Result[int, MathError] = compute()\n    match outcome:\n        Result.Ok(value):\n            return value\n        Result.Err(error):\n            match error:\n                MathError.TooSmall:\n                    return 0\n                MathError.NotEven(value):\n                    return value\n",
        );
        let compiled =
            compile_source(&source, CompileMode::Executable).expect("compile should succeed");

        let result_type = Type::Result(
            Box::new(Type::Int),
            Box::new(Type::Enum("MathError".to_string())),
        );
        assert_eq!(compiled.typed_hir.functions[0].return_type, result_type);
        assert_eq!(compiled.typed_hir.functions[1].return_type, result_type);

        assert!(
            compiled.ssa.functions[1]
                .values
                .iter()
                .any(|value| matches!(value.instruction, SsaInstruction::Propagate { .. }))
        );
    }
}
