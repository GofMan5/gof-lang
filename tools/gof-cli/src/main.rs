use anyhow::{Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use gof_compiler::{
    CompileMode, Diagnostics, SourceFile, build_embedded_source_bundle, compile_source,
    format_source, normalize_source_path,
    package::{
        PackageGraphError, PackageManifestError, ensure_fresh_lockfile_for_source,
        find_package_root, package_library_entry_path, package_main_entry_path,
        write_lockfile_for_directory,
    },
    run_module_with_output, run_module_with_output_and_args,
};
use gof_runtime::profile;
use serde_json::json;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

mod test_runner;
mod watch;

#[derive(Parser)]
#[command(name = "gof", about = "Bootstrap toolchain for the gof language")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build(BuildArgs),
    Check(CheckArgs),
    Run(FileInput),
    Test(TestArgs),
    Fmt(FmtArgs),
    Mod {
        #[command(subcommand)]
        command: ModCommand,
    },
    Doc,
    Bench,
}

#[derive(Args)]
struct FileInput {
    input: PathBuf,
    #[arg(short = 'w', long)]
    watch: bool,
    #[arg(long, default_value_t = 150)]
    debounce_ms: u64,
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct RunTarget {
    entry_path: PathBuf,
    package_root: Option<PathBuf>,
    compile_mode: CompileMode,
}

#[derive(Args)]
struct BuildArgs {
    input: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long)]
    native: bool,
}

#[derive(Args)]
struct CheckArgs {
    input: PathBuf,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    stdin: bool,
}

#[derive(Args)]
struct FmtArgs {
    input: PathBuf,
    #[arg(long)]
    check: bool,
}

#[derive(Args)]
pub(crate) struct TestArgs {
    #[arg()]
    paths: Vec<PathBuf>,
    #[arg(long)]
    filter: Option<String>,
    #[arg(long)]
    exact: bool,
    #[arg(long)]
    list: bool,
    #[arg(long)]
    fail_fast: bool,
    #[arg(long)]
    nocapture: bool,
    #[arg(long)]
    update_snapshots: bool,
    #[arg(long)]
    docs: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum ModCommand {
    Init(ModInitArgs),
    Resolve(ModResolveArgs),
}

#[derive(Args)]
struct ModInitArgs {
    module: String,
    #[arg(long, default_value = "2026")]
    edition: String,
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

#[derive(Args)]
struct ModResolveArgs {
    #[arg(long, default_value = ".")]
    dir: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build(args) => build(args),
        Command::Check(args) => check_file(args),
        Command::Run(args) => run_file(args),
        Command::Test(args) => test_runner::run(args),
        Command::Fmt(args) => format_file(args),
        Command::Mod { command } => match command {
            ModCommand::Init(args) => init_module(args),
            ModCommand::Resolve(args) => resolve_module_lockfile(args),
        },
        Command::Doc => print_docs(),
        Command::Bench => print_benchmarks(),
    }
}

fn build(args: BuildArgs) -> Result<()> {
    let resolved_input = resolve_source_input_path(&args.input)?;
    ensure_package_lockfile(&resolved_input, "build")?;
    let source = SourceFile::from_path(&resolved_input)?;
    let compiled = compile_source(&source, CompileMode::Executable)
        .map_err(|error| render_error(&source, error))?;
    let output = if args.native {
        default_native_build_path(&resolved_input, args.output)
    } else {
        args.output
            .clone()
            .unwrap_or_else(|| default_build_path(&resolved_input))
    };

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    if args.native {
        build_native_host_executable(&source, &output)?;
        println!("wrote native executable {}", output.display());
    } else {
        fs::write(&output, serde_json::to_string_pretty(&compiled.backend)?)?;
        println!("wrote {}", output.display());
    }
    Ok(())
}

fn check_file(args: CheckArgs) -> Result<()> {
    if args.stdin && args.input.is_dir() {
        bail!("`gof check --stdin` expects a file path, not a package directory");
    }

    let resolved_input = resolve_source_input_path(&args.input)?;
    if let Err(error) = ensure_fresh_lockfile_for_source(&resolved_input) {
        if args.json {
            emit_json_check_report(
                &resolved_input,
                false,
                vec![json!({
                    "code": package_error_code(&error),
                    "severity": "error",
                    "message": render_package_error_with_operation(error, "check").to_string(),
                    "note": "resolve the package error and rerun `gof check`",
                    "fixIt": serde_json::Value::Null,
                    "sourcePath": display_path(&resolved_input),
                    "line": 1,
                    "column": 1,
                    "endColumn": 1
                })],
            )?;
            std::process::exit(1);
        }
        return Err(render_package_error_with_operation(error, "check"));
    }

    let source = match load_check_source(&resolved_input, args.stdin) {
        Ok(source) => source,
        Err(error) if args.json => {
            emit_json_check_report(
                &resolved_input,
                false,
                vec![json!({
                    "code": "GOFIO",
                    "severity": "error",
                    "message": format!("failed to read source file: {error}"),
                    "note": "ensure the file exists and is readable",
                    "fixIt": serde_json::Value::Null,
                    "sourcePath": display_path(&resolved_input),
                    "line": 1,
                    "column": 1,
                    "endColumn": 1
                })],
            )?;
            std::process::exit(1);
        }
        Err(error) => return Err(error.into()),
    };

    let mode = compile_mode_for_entry_path(&resolved_input);
    match compile_source(&source, mode) {
        Ok(_) => {
            if args.json {
                emit_json_check_report(&resolved_input, true, Vec::new())?;
            }
            Ok(())
        }
        Err(error) if args.json => {
            emit_json_check_report(&resolved_input, false, diagnostics_to_json(&source, &error))?;
            std::process::exit(1);
        }
        Err(error) => Err(render_error(&source, error)),
    }
}

fn run_file(args: FileInput) -> Result<()> {
    let target = resolve_run_target(&args.input)?;
    if args.watch {
        if target.compile_mode != CompileMode::Executable {
            return Err(render_watch_target_error(&target));
        }
        return watch::run_watch_loop(target, args.args, args.debounce_ms);
    }

    execute_run_target(&target, &args.args)
}

fn format_file(args: FmtArgs) -> Result<()> {
    let source = SourceFile::from_path(&args.input)?;
    let formatted = format_source(&source).map_err(|error| render_error(&source, error))?;

    if args.check {
        if formatted == source.text() {
            println!("already formatted");
            return Ok(());
        }
        bail!("file is not formatted: {}", args.input.display());
    }

    fs::write(&args.input, formatted)?;
    println!("formatted {}", args.input.display());
    Ok(())
}

fn init_module(args: ModInitArgs) -> Result<()> {
    fs::create_dir_all(&args.dir)?;
    let manifest_path = args.dir.join("gof.mod");
    if manifest_path.exists() {
        bail!("manifest already exists: {}", manifest_path.display());
    }

    let manifest = format!(
        "module = \"{}\"\nedition = \"{}\"\n\n[dependencies]\n",
        args.module, args.edition
    );
    fs::write(&manifest_path, manifest)?;
    let source_root = args.dir.join("src");
    fs::create_dir_all(&source_root)?;
    let main_path = source_root.join("main.gof");
    fs::write(&main_path, "fn main() -> int:\n    return 0\n")?;
    println!("created {}", manifest_path.display());
    println!("created {}", main_path.display());
    Ok(())
}

fn resolve_module_lockfile(args: ModResolveArgs) -> Result<()> {
    let lockfile_path = write_lockfile_for_directory(&args.dir).map_err(render_package_error)?;
    println!("wrote {}", display_path(&lockfile_path));
    Ok(())
}

fn resolve_source_input_path(input: &Path) -> Result<PathBuf> {
    if input.is_dir() {
        let manifest_path = input.join("gof.mod");
        if !manifest_path.is_file() {
            bail!(
                "package directory is missing manifest: {}",
                manifest_path.display()
            );
        }

        let entry_path = package_main_entry_path(input);
        if !entry_path.is_file() {
            bail!(
                "package directory is missing executable entrypoint: {}",
                entry_path.display()
            );
        }

        return Ok(entry_path);
    }

    Ok(input.to_path_buf())
}

fn resolve_run_target(input: &Path) -> Result<RunTarget> {
    let resolved_input = resolve_source_input_path(input)?;
    Ok(RunTarget {
        package_root: find_package_root(&resolved_input),
        compile_mode: compile_mode_for_entry_path(&resolved_input),
        entry_path: normalize_source_path(&resolved_input),
    })
}

fn resolve_package_test_input(input: &Path) -> Result<Option<PathBuf>> {
    if input.is_file() {
        return Ok(Some(input.to_path_buf()));
    }

    let Some(package_root) = find_package_root(input) else {
        return Ok(None);
    };
    if normalize_cli_path(&package_root) != normalize_cli_path(input) {
        return Ok(None);
    }

    let main_entry = package_main_entry_path(&package_root);
    if main_entry.is_file() {
        return Ok(Some(main_entry));
    }

    let library_entry = package_library_entry_path(&package_root);
    if library_entry.is_file() {
        return Ok(Some(library_entry));
    }

    bail!(
        "package directory is missing a testable entrypoint: {} or {}",
        package_main_entry_path(&package_root).display(),
        package_library_entry_path(&package_root).display()
    );
}

fn run_package_test(entry_path: &Path) -> Result<()> {
    let report = run_package_test_report(entry_path);
    if !report.stdout.is_empty() {
        print!("{}", report.stdout);
    }
    if let Some(error) = report.failure_message {
        bail!(error);
    }
    Ok(())
}

pub(crate) fn run_package_test_report(entry_path: &Path) -> PackageTestReport {
    let source = match SourceFile::from_path(entry_path) {
        Ok(source) => source,
        Err(error) => {
            return PackageTestReport {
                stdout: String::new(),
                stderr: error.to_string(),
                diagnostics: Vec::new(),
                failure_message: Some(error.to_string()),
            };
        }
    };
    let mode = compile_mode_for_entry_path(entry_path);
    match mode {
        CompileMode::Library => match compile_source(&source, mode) {
            Ok(_) => PackageTestReport {
                stdout: String::new(),
                stderr: String::new(),
                diagnostics: Vec::new(),
                failure_message: None,
            },
            Err(error) => {
                let stderr = render_error(&source, error.clone()).to_string();
                PackageTestReport {
                    stdout: String::new(),
                    stderr: stderr.clone(),
                    diagnostics: diagnostics_to_json(&source, &error),
                    failure_message: Some(stderr),
                }
            }
        },
        CompileMode::Executable => match run_module_with_output(&source) {
            Ok(result) => PackageTestReport {
                stdout: render_execution_stdout(&result),
                stderr: String::new(),
                diagnostics: Vec::new(),
                failure_message: None,
            },
            Err(error) => {
                let stderr = render_error(&source, error.clone()).to_string();
                PackageTestReport {
                    stdout: String::new(),
                    stderr: stderr.clone(),
                    diagnostics: diagnostics_to_json(&source, &error),
                    failure_message: Some(stderr),
                }
            }
        },
    }
}

fn execute_run_target(target: &RunTarget, program_args: &[String]) -> Result<()> {
    ensure_package_lockfile(&target.entry_path, "run")?;
    let source = SourceFile::from_path(&target.entry_path)?;
    let result = run_module_with_output_and_args(&source, program_args)
        .map_err(|error| render_error(&source, error))?;
    render_execution_result(&result);
    Ok(())
}

fn render_execution_result(result: &gof_compiler::ExecutionResult) {
    if !result.stdout.is_empty() {
        print!("{}", result.stdout);
    }
    if let Some(rendered) = result.value.cli_text() {
        println!("{rendered}");
    }
}

fn ensure_package_lockfile(source_path: &Path, operation: &str) -> Result<()> {
    ensure_fresh_lockfile_for_source(source_path)
        .map_err(|error| render_package_error_with_operation(error, operation))?;
    Ok(())
}

fn print_docs() -> Result<()> {
    println!("language: spec/language-v1.md");
    println!("diagnostics: spec/diagnostics.md");
    println!("packages: spec/package-system.md");
    println!(
        "runtime profile: {}",
        serde_json::to_string_pretty(&profile())?
    );
    Ok(())
}

fn print_benchmarks() -> Result<()> {
    println!("benchmark crate: benchmarks/gof-bench");
    println!("run with: cargo bench -p gof-bench");
    Ok(())
}

fn compile_mode_for_entry_path(path: &Path) -> CompileMode {
    if path.file_name().and_then(|value| value.to_str()) == Some("lib.gof") {
        CompileMode::Library
    } else {
        CompileMode::Executable
    }
}

fn load_check_source(path: &Path, read_stdin: bool) -> Result<SourceFile> {
    if read_stdin {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        Ok(SourceFile::new(path.to_path_buf(), text))
    } else {
        Ok(SourceFile::from_path(path)?)
    }
}

fn diagnostics_to_json(source: &SourceFile, diagnostics: &Diagnostics) -> Vec<serde_json::Value> {
    diagnostics
        .0
        .iter()
        .map(|diagnostic| {
            let severity = match diagnostic.severity {
                gof_compiler::Severity::Error => "error",
                gof_compiler::Severity::Warning => "warning",
            };
            let source_path = diagnostic
                .source_path
                .as_deref()
                .unwrap_or_else(|| source.path());
            json!({
                "code": diagnostic.code,
                "severity": severity,
                "message": diagnostic.message,
                "note": diagnostic.note,
                "fixIt": diagnostic.fix_it,
                "sourcePath": display_path(source_path),
                "line": diagnostic.span.line,
                "column": diagnostic.span.column,
                "endColumn": diagnostic.span.end_column
            })
        })
        .collect()
}

fn emit_json_check_report(
    input: &Path,
    ok: bool,
    diagnostics: Vec<serde_json::Value>,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": ok,
            "input": display_path(input),
            "diagnostics": diagnostics
        }))?
    );
    Ok(())
}

fn default_build_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("module");
    PathBuf::from("target")
        .join("gof")
        .join(format!("{stem}.ssa.json"))
}

fn default_native_build_path(input: &Path, output: Option<PathBuf>) -> PathBuf {
    match output {
        Some(output) => with_platform_executable_extension(output),
        None => {
            let stem = input
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("module");
            with_platform_executable_extension(PathBuf::from("target").join("gof").join(stem))
        }
    }
}

fn with_platform_executable_extension(path: PathBuf) -> PathBuf {
    if cfg!(windows) && path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path
    }
}

fn build_native_host_executable(source: &SourceFile, output: &Path) -> Result<()> {
    let workspace_root = workspace_root()?;
    let compiler_crate = workspace_root.join("compiler").join("gof-compiler");
    if !compiler_crate.exists() {
        bail!(
            "bootstrap native build requires the compiler crate source at {}",
            compiler_crate.display()
        );
    }

    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("gof-program");
    let package_name = sanitize_package_name(stem);
    let build_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let project_dir = workspace_root
        .join("target")
        .join("gof-native")
        .join(format!("{package_name}-{build_id}"));
    let src_dir = project_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    let compiler_path = compiler_crate.to_string_lossy().replace('\\', "/");
    let embedded_bundle =
        build_embedded_source_bundle(source).map_err(|error| render_error(source, error))?;
    let serialized_bundle = serde_json::to_string(&embedded_bundle)?;
    let cargo_toml = format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\ngof-compiler = {{ path = \"{compiler_path}\" }}\nserde_json = \"1.0\"\n\n[workspace]\n"
    );
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;

    let runner = format!(
        "use gof_compiler::{{EmbeddedSourceBundle, run_embedded_bundle_with_output_and_args}};\n\nconst EMBEDDED_BUNDLE: &str = {serialized_bundle:?};\n\nfn main() {{\n    let bundle: EmbeddedSourceBundle = serde_json::from_str(EMBEDDED_BUNDLE)\n        .expect(\"embedded source bundle should deserialize\");\n    let program_args = std::env::args().skip(1).collect::<Vec<_>>();\n    match run_embedded_bundle_with_output_and_args(&bundle, &program_args) {{\n        Ok(result) => {{\n            if !result.stdout.is_empty() {{\n                print!(\"{{}}\", result.stdout);\n            }}\n            if let Some(rendered) = result.value.cli_text() {{\n                println!(\"{{rendered}}\");\n            }}\n        }}\n        Err(error) => {{\n            let source = bundle\n                .entry_source()\n                .expect(\"embedded source bundle should contain entry source\");\n            eprintln!(\"{{}}\", error.render(&source));\n            std::process::exit(1);\n        }}\n    }}\n}}\n"
    );
    fs::write(src_dir.join("main.rs"), runner)?;

    let status = ProcessCommand::new("cargo")
        .args(["build", "--release", "--offline", "--manifest-path"])
        .arg(project_dir.join("Cargo.toml"))
        .status()?;
    if !status.success() {
        bail!("cargo build --release --offline failed for bootstrap native build");
    }

    let built_binary = with_platform_executable_extension(
        project_dir
            .join("target")
            .join("release")
            .join(&package_name),
    );
    if !built_binary.exists() {
        bail!(
            "bootstrap native build did not produce {}",
            built_binary.display()
        );
    }

    fs::copy(&built_binary, output)?;
    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("unable to resolve workspace root from CLI crate path"))
}

fn sanitize_package_name(stem: &str) -> String {
    let mut value = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        value = "gof-program".to_string();
    }
    if value
        .chars()
        .next()
        .map(|character| character.is_ascii_digit())
        .unwrap_or(false)
    {
        value.insert(0, 'g');
    }
    value
}

fn normalize_cli_path(path: &Path) -> String {
    let normalized = normalize_source_path(path)
        .to_string_lossy()
        .replace("\\\\?\\", "")
        .replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureHarnessKind {
    LegacyPass,
    LegacyCompileFail,
    LegacyRuntimeFail,
    UiCompileFail,
    RuntimePass,
    RuntimeFail,
}

#[derive(Debug, Clone)]
struct FixtureArtifacts {
    stdout: String,
    stderr: String,
    exit_code: i32,
    diag_codes: Vec<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) struct FixtureRunReport {
    pub stdout: String,
    pub stderr: String,
    pub diagnostics: Vec<serde_json::Value>,
    pub failure_message: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PackageTestReport {
    pub stdout: String,
    pub stderr: String,
    pub diagnostics: Vec<serde_json::Value>,
    pub failure_message: Option<String>,
}

fn classify_fixture_harness(path: &Path) -> FixtureHarnessKind {
    let in_legacy_fixture_tree = path
        .components()
        .any(|component| component.as_os_str() == "fixtures");
    if in_legacy_fixture_tree
        && path
            .components()
            .any(|component| component.as_os_str() == "runtime-fail")
    {
        FixtureHarnessKind::LegacyRuntimeFail
    } else if in_legacy_fixture_tree
        && path
            .components()
            .any(|component| component.as_os_str() == "fail")
    {
        FixtureHarnessKind::LegacyCompileFail
    } else if path
        .components()
        .any(|component| component.as_os_str() == "runtime-fail")
    {
        FixtureHarnessKind::RuntimeFail
    } else if path
        .components()
        .any(|component| component.as_os_str() == "runtime")
    {
        FixtureHarnessKind::RuntimePass
    } else if path.components().any(|component| component.as_os_str() == "ui") {
        FixtureHarnessKind::UiCompileFail
    } else if path
        .components()
        .any(|component| component.as_os_str() == "fail")
    {
        FixtureHarnessKind::LegacyCompileFail
    } else {
        FixtureHarnessKind::LegacyPass
    }
}

fn run_fixture(path: &Path, update_expected_artifacts: bool) -> Result<()> {
    let report = run_fixture_report(path, update_expected_artifacts);
    if let Some(error) = report.failure_message {
        bail!(error);
    }
    Ok(())
}

pub(crate) fn run_fixture_report(path: &Path, update_expected_artifacts: bool) -> FixtureRunReport {
    let harness = classify_fixture_harness(path);
    if matches!(
        harness,
        FixtureHarnessKind::UiCompileFail
            | FixtureHarnessKind::RuntimePass
            | FixtureHarnessKind::RuntimeFail
    ) {
        return run_product_fixture_report(path, harness, update_expected_artifacts);
    }

    let source = match SourceFile::from_path(path) {
        Ok(source) => source,
        Err(error) => {
            return FixtureRunReport {
                stdout: String::new(),
                stderr: error.to_string(),
                diagnostics: Vec::new(),
                failure_message: Some(error.to_string()),
            };
        }
    };

    match compile_source(&source, CompileMode::Executable) {
        Ok(_) if harness == FixtureHarnessKind::LegacyCompileFail => FixtureRunReport {
            stdout: String::new(),
            stderr: "expected fixture to fail during compilation".to_string(),
            diagnostics: Vec::new(),
            failure_message: Some("expected fixture to fail during compilation".to_string()),
        },
        Ok(_) if harness == FixtureHarnessKind::LegacyRuntimeFail => match run_module_with_output(&source) {
            Ok(result) => {
                let stdout = render_execution_stdout(&result);
                let message = format!(
                    "expected runtime failure, but program succeeded with stdout {:?} and value {:?}",
                    result.stdout,
                    result.value.cli_text()
                );
                FixtureRunReport {
                    stdout,
                    stderr: message.clone(),
                    diagnostics: Vec::new(),
                    failure_message: Some(message),
                }
            }
            Err(error) => {
                let stderr = render_fixture_diagnostics(path, &source, &error);
                let failure_message = validate_expected_diagnostics(path, &error, update_expected_artifacts)
                    .err()
                    .map(|error| error.to_string());
                FixtureRunReport {
                    stdout: String::new(),
                    stderr,
                    diagnostics: diagnostics_to_json(&source, &error),
                    failure_message,
                }
            }
        },
        Ok(_) => FixtureRunReport {
            stdout: String::new(),
            stderr: String::new(),
            diagnostics: Vec::new(),
            failure_message: None,
        },
        Err(error) if harness == FixtureHarnessKind::LegacyCompileFail => {
            let stderr = render_fixture_diagnostics(path, &source, &error);
            let failure_message = validate_expected_diagnostics(path, &error, update_expected_artifacts)
                .err()
                .map(|error| error.to_string());
            FixtureRunReport {
                stdout: String::new(),
                stderr,
                diagnostics: diagnostics_to_json(&source, &error),
                failure_message,
            }
        }
        Err(error) if harness == FixtureHarnessKind::LegacyRuntimeFail => {
            let rendered = render_error(&source, error.clone()).to_string();
            FixtureRunReport {
                stdout: String::new(),
                stderr: rendered.clone(),
                diagnostics: diagnostics_to_json(&source, &error),
                failure_message: Some(format!(
                    "expected runtime failure, but compilation failed instead:\n{rendered}"
                )),
            }
        }
        Err(error) => {
            let rendered = render_error(&source, error.clone()).to_string();
            FixtureRunReport {
                stdout: String::new(),
                stderr: rendered.clone(),
                diagnostics: diagnostics_to_json(&source, &error),
                failure_message: Some(rendered),
            }
        }
    }
}

fn run_product_fixture_report(
    path: &Path,
    harness: FixtureHarnessKind,
    update_expected_artifacts: bool,
) -> FixtureRunReport {
    let source = match SourceFile::from_path(path) {
        Ok(source) => source,
        Err(error) => {
            return FixtureRunReport {
                stdout: String::new(),
                stderr: error.to_string(),
                diagnostics: Vec::new(),
                failure_message: Some(error.to_string()),
            };
        }
    };
    let (actual, diagnostics, failure_message) = match harness {
        FixtureHarnessKind::UiCompileFail => match compile_source(&source, CompileMode::Executable) {
            Ok(_) => (
                FixtureArtifacts {
                    stdout: String::new(),
                    stderr: "expected ui fixture to fail during compilation".to_string(),
                    exit_code: 0,
                    diag_codes: Vec::new(),
                },
                Vec::new(),
                Some("expected ui fixture to fail during compilation".to_string()),
            ),
            Err(error) => (
                FixtureArtifacts {
                    stdout: String::new(),
                    stderr: render_fixture_diagnostics(path, &source, &error),
                    exit_code: 1,
                    diag_codes: error.codes(),
                },
                diagnostics_to_json(&source, &error),
                None,
            ),
        },
        FixtureHarnessKind::RuntimePass => match run_module_with_output(&source) {
            Ok(result) => (
                FixtureArtifacts {
                    stdout: render_execution_stdout(&result),
                    stderr: String::new(),
                    exit_code: 0,
                    diag_codes: Vec::new(),
                },
                Vec::new(),
                None,
            ),
            Err(error) => {
                let stderr = render_fixture_diagnostics(path, &source, &error);
                (
                    FixtureArtifacts {
                        stdout: String::new(),
                        stderr: stderr.clone(),
                        exit_code: 1,
                        diag_codes: error.codes(),
                    },
                    diagnostics_to_json(&source, &error),
                    Some(format!(
                        "expected runtime fixture to succeed, but it failed instead:\n{stderr}"
                    )),
                )
            }
        },
        FixtureHarnessKind::RuntimeFail => match run_module_with_output(&source) {
            Ok(result) => (
                FixtureArtifacts {
                    stdout: render_execution_stdout(&result),
                    stderr: String::new(),
                    exit_code: 0,
                    diag_codes: Vec::new(),
                },
                Vec::new(),
                Some(format!(
                    "expected runtime failure, but program succeeded with stdout {:?} and value {:?}",
                    result.stdout,
                    result.value.cli_text()
                )),
            ),
            Err(error) => (
                FixtureArtifacts {
                    stdout: String::new(),
                    stderr: render_fixture_diagnostics(path, &source, &error),
                    exit_code: 1,
                    diag_codes: error.codes(),
                },
                diagnostics_to_json(&source, &error),
                None,
            ),
        },
        FixtureHarnessKind::LegacyPass
        | FixtureHarnessKind::LegacyCompileFail
        | FixtureHarnessKind::LegacyRuntimeFail => (
            FixtureArtifacts {
                stdout: String::new(),
                stderr: "product fixture runner received a legacy fixture kind".to_string(),
                exit_code: 1,
                diag_codes: Vec::new(),
            },
            Vec::new(),
            Some("product fixture runner received a legacy fixture kind".to_string()),
        ),
    };

    let failure_message = failure_message.or_else(|| {
        validate_product_fixture_artifacts(path, harness, &actual, update_expected_artifacts)
            .err()
            .map(|error| error.to_string())
    });

    FixtureRunReport {
        stdout: actual.stdout,
        stderr: actual.stderr,
        diagnostics,
        failure_message,
    }
}

fn render_execution_stdout(result: &gof_compiler::ExecutionResult) -> String {
    let mut rendered = result.stdout.clone();
    if let Some(value) = result.value.cli_text() {
        rendered.push_str(&value);
        rendered.push('\n');
    }
    normalize_artifact_text(&rendered)
}

fn render_fixture_diagnostics(path: &Path, source: &SourceFile, diagnostics: &Diagnostics) -> String {
    let rendered = diagnostics.render(source);
    normalize_fixture_output_paths(path, &rendered)
}

fn validate_product_fixture_artifacts(
    path: &Path,
    harness: FixtureHarnessKind,
    actual: &FixtureArtifacts,
    update_expected_artifacts: bool,
) -> Result<()> {
    match harness {
        FixtureHarnessKind::UiCompileFail | FixtureHarnessKind::RuntimeFail => {
            sync_fixture_artifact(path, "stderr", &actual.stderr, update_expected_artifacts)?;
            sync_fixture_artifact(
                path,
                "diag",
                &diag_artifact_text(&actual.diag_codes),
                update_expected_artifacts,
            )?;
            sync_fixture_artifact(
                path,
                "exit",
                &format!("{}\n", actual.exit_code),
                update_expected_artifacts,
            )?;
            remove_fixture_artifact(path, "stdout", update_expected_artifacts)?;
        }
        FixtureHarnessKind::RuntimePass => {
            sync_fixture_artifact(path, "stdout", &actual.stdout, update_expected_artifacts)?;
            sync_fixture_artifact(
                path,
                "exit",
                &format!("{}\n", actual.exit_code),
                update_expected_artifacts,
            )?;
            remove_fixture_artifact(path, "stderr", update_expected_artifacts)?;
            remove_fixture_artifact(path, "diag", update_expected_artifacts)?;
        }
        FixtureHarnessKind::LegacyPass
        | FixtureHarnessKind::LegacyCompileFail
        | FixtureHarnessKind::LegacyRuntimeFail => {}
    }

    Ok(())
}

fn validate_expected_diagnostics(
    path: &Path,
    error: &Diagnostics,
    update_expected_artifacts: bool,
) -> Result<()> {
    sync_fixture_artifact(
        path,
        "diag",
        &diag_artifact_text(&error.codes()),
        update_expected_artifacts,
    )
}

fn diag_artifact_text(codes: &[&str]) -> String {
    if codes.is_empty() {
        String::new()
    } else {
        format!("{}\n", codes.join("\n"))
    }
}

fn sync_fixture_artifact(
    path: &Path,
    extension: &str,
    actual: &str,
    update_expected_artifacts: bool,
) -> Result<()> {
    let artifact_path = path.with_extension(extension);
    let actual = normalize_artifact_text(actual);

    match fs::read_to_string(&artifact_path) {
        Ok(expected) => {
            if normalize_artifact_text(&expected) != actual {
                if update_expected_artifacts {
                    fs::write(&artifact_path, actual)?;
                    return Ok(());
                }
                bail!(
                    "fixture artifact mismatch for `{}`\n  note: artifact `{}` differs from the current output\n  help: rerun `gof test --update-snapshots {}` to accept the new artifact",
                    fixture_display_id(path),
                    display_path(&artifact_path),
                    display_path(path)
                );
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if update_expected_artifacts {
                fs::write(&artifact_path, actual)?;
                Ok(())
            } else {
                bail!(
                    "missing expected fixture artifact `{}` for `{}`\n  help: rerun `gof test --update-snapshots {}` to create it",
                    display_path(&artifact_path),
                    fixture_display_id(path),
                    display_path(path)
                );
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn remove_fixture_artifact(path: &Path, extension: &str, update_expected_artifacts: bool) -> Result<()> {
    if !update_expected_artifacts {
        return Ok(());
    }

    let artifact_path = path.with_extension(extension);
    if artifact_path.exists() {
        fs::remove_file(artifact_path)?;
    }
    Ok(())
}

fn normalize_artifact_text(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn normalize_fixture_output_paths(path: &Path, text: &str) -> String {
    let mut normalized = normalize_artifact_text(text);
    let source_path = normalize_source_path(path);
    let project_root = fixture_project_root(path);
    let display_id = fixture_display_id(path);

    for spelling in path_spellings(&source_path) {
        normalized = normalized.replace(&spelling, &display_id);
    }

    for spelling in path_spellings(&project_root) {
        for prefix in [format!("{spelling}\\"), format!("{spelling}/")] {
            normalized = normalized.replace(&prefix, "");
        }
    }

    normalized
}

fn fixture_project_root(path: &Path) -> PathBuf {
    let normalized = normalize_source_path(path);
    for ancestor in normalized.ancestors() {
        if matches!(
            ancestor.file_name().and_then(|value| value.to_str()),
            Some("tests" | "src")
        ) {
            return ancestor
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
        }
    }

    normalized
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn fixture_display_id(path: &Path) -> String {
    let normalized = normalize_source_path(path);
    let project_root = fixture_project_root(&normalized);
    normalized
        .strip_prefix(&project_root)
        .map(display_path)
        .unwrap_or_else(|_| display_path(&normalized))
        .replace('\\', "/")
}

fn path_spellings(path: &Path) -> Vec<String> {
    let rendered = display_path(path);
    let unix = rendered.replace('\\', "/");
    let windows = rendered.replace('/', "\\");
    let mut values = vec![
        rendered,
        unix.clone(),
        windows,
        unix.replace('\\', "/"),
    ];
    if cfg!(windows) {
        values.push(unix.to_ascii_lowercase());
    }
    values.sort();
    values.dedup();
    values
}

fn render_error(source: &SourceFile, error: Diagnostics) -> anyhow::Error {
    anyhow!(error.render(source))
}

fn render_package_error(error: PackageGraphError) -> anyhow::Error {
    render_package_error_with_operation(error, "resolve")
}

fn render_package_error_with_operation(error: PackageGraphError, operation: &str) -> anyhow::Error {
    match error {
        PackageGraphError::Manifest(error) => anyhow!(render_manifest_error(error)),
        PackageGraphError::MissingLockfile {
            module,
            package_root,
            path,
        } => anyhow!(format!(
            "error[GOF3090]: manifest-backed package `{module}` is missing a lockfile for `{operation}`\n  note: expected {}\n  note: package root {}\n  help: run `gof mod resolve --dir {}`",
            display_path(&path),
            display_path(&package_root),
            display_path(&package_root)
        )),
        PackageGraphError::StaleLockfile {
            module,
            package_root,
            path,
            reason,
        } => anyhow!(format!(
            "error[GOF3091]: lockfile for manifest-backed package `{module}` is stale for `{operation}`\n  note: {}\n  note: lockfile {}\n  note: package root {}\n  help: run `gof mod resolve --dir {}`",
            reason,
            display_path(&path),
            display_path(&package_root),
            display_path(&package_root)
        )),
        PackageGraphError::ConflictingModuleIdentity {
            module,
            existing_root,
            new_root,
        } => anyhow!(format!(
            "error[GOF3092]: conflicting package identity in the local dependency graph for `{module}`\n  note: first root {}\n  note: second root {}\n  help: make each local package module name unique",
            display_path(&existing_root),
            display_path(&new_root)
        )),
        PackageGraphError::ConflictingPackageMetadata {
            package_root,
            existing_module,
            existing_edition,
            new_module,
            new_edition,
        } => anyhow!(format!(
            "error[GOF3092]: conflicting package metadata for `{}`\n  note: expected module `{existing_module}` edition `{existing_edition}`\n  note: got module `{new_module}` edition `{new_edition}`\n  help: keep one stable manifest identity per package root",
            display_path(&package_root)
        )),
        PackageGraphError::LockfileParse { path, message } => anyhow!(format!(
            "error[GOF3091]: failed to parse lockfile `{}`\n  note: {}\n  help: run `gof mod resolve --dir {}`",
            display_path(&path),
            message,
            display_path(path.parent().unwrap_or_else(|| Path::new(".")))
        )),
        PackageGraphError::LockfileRead { path, message } => anyhow!(format!(
            "error[GOF3091]: failed to read lockfile `{}`\n  note: {}\n  help: ensure the lockfile is readable and rerun `gof mod resolve`",
            display_path(&path),
            message
        )),
        PackageGraphError::DependencyCycle { cycle } => anyhow!(format!(
            "error: local package dependency cycle detected during `{operation}`\n  note: {}\n  help: break the cycle by extracting shared code into an acyclic package",
            cycle
                .iter()
                .map(|path| display_path(path))
                .collect::<Vec<_>>()
                .join(" -> ")
        )),
        PackageGraphError::MissingEntrypoint {
            module,
            package_root,
            expected_entry,
        } => anyhow!(format!(
            "error: package `{module}` is missing the required entrypoint `{}`\n  note: package root {}\n  help: add the expected package entrypoint before rerunning `{operation}`",
            display_path(&expected_entry),
            display_path(&package_root)
        )),
        PackageGraphError::MissingRootManifest { start } => anyhow!(format!(
            "error: no `gof.mod` manifest was found starting from {}\n  help: run `gof mod init` first or point `--dir` at an existing package root",
            display_path(&start)
        )),
    }
}

fn render_watch_target_error(target: &RunTarget) -> anyhow::Error {
    anyhow!(format!(
        "error[GOF3102]: `gof run --watch` requires an executable target\n  note: `{}` resolves to a library entrypoint\n  help: watch a single-file script or a package with `src/main.gof`",
        display_path(&target.entry_path)
    ))
}

fn package_error_code(error: &PackageGraphError) -> &'static str {
    match error {
        PackageGraphError::Manifest(_) => "GOF3089",
        PackageGraphError::MissingLockfile { .. } => "GOF3090",
        PackageGraphError::StaleLockfile { .. }
        | PackageGraphError::LockfileParse { .. }
        | PackageGraphError::LockfileRead { .. } => "GOF3091",
        PackageGraphError::ConflictingModuleIdentity { .. }
        | PackageGraphError::ConflictingPackageMetadata { .. } => "GOF3092",
        PackageGraphError::DependencyCycle { .. } => "GOF3089",
        PackageGraphError::MissingEntrypoint { .. } => "GOF3089",
        PackageGraphError::MissingRootManifest { .. } => "GOF3089",
    }
}

fn render_manifest_error(error: PackageManifestError) -> String {
    match error {
        PackageManifestError::Read { path, message } => format!(
            "error[GOF3089]: invalid package manifest\n  note: failed to read {}\n  note: {}\n  help: repair `gof.mod` or remove the broken local package configuration",
            display_path(&path),
            message
        ),
        PackageManifestError::Parse { path, message } => format!(
            "error[GOF3089]: invalid package manifest\n  note: failed to parse {}\n  note: {}\n  help: fix the TOML syntax in `gof.mod`",
            display_path(&path),
            message
        ),
        PackageManifestError::MissingDependencyManifest {
            manifest,
            dependency,
            expected_manifest,
        } => format!(
            "error[GOF3089]: invalid local dependency `{dependency}`\n  note: {} declares `{dependency}` but {} does not exist\n  help: point the dependency at a directory that contains a valid `gof.mod`",
            display_path(&manifest),
            display_path(&expected_manifest)
        ),
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string().replace("\\\\?\\", "")
}
