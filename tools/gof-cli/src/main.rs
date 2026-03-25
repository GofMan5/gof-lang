use anyhow::{Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use gof_compiler::{
    CompileMode, Diagnostics, SourceFile, compile_source, format_source, run_module,
};
use gof_runtime::profile;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "gof", about = "Bootstrap toolchain for the gof language")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build(FileInput),
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
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct FmtArgs {
    input: PathBuf,
    #[arg(long)]
    check: bool,
}

#[derive(Args)]
struct TestArgs {
    #[arg(default_value = "tests/fixtures")]
    fixtures: PathBuf,
}

#[derive(Subcommand)]
enum ModCommand {
    Init(ModInitArgs),
}

#[derive(Args)]
struct ModInitArgs {
    module: String,
    #[arg(long, default_value = "2026")]
    edition: String,
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
        Command::Run(args) => run_file(args),
        Command::Test(args) => test_fixtures(args),
        Command::Fmt(args) => format_file(args),
        Command::Mod {
            command: ModCommand::Init(args),
        } => init_module(args),
        Command::Doc => print_docs(),
        Command::Bench => print_benchmarks(),
    }
}

fn build(args: FileInput) -> Result<()> {
    let source = SourceFile::from_path(&args.input)?;
    let compiled = compile_source(&source, CompileMode::Executable)
        .map_err(|error| render_error(&source, error))?;
    let output = args
        .output
        .unwrap_or_else(|| default_build_path(&args.input));

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, serde_json::to_string_pretty(&compiled.backend)?)?;
    println!("wrote {}", output.display());
    Ok(())
}

fn run_file(args: FileInput) -> Result<()> {
    let source = SourceFile::from_path(&args.input)?;
    let value = run_module(&source).map_err(|error| render_error(&source, error))?;
    if let Some(rendered) = value.cli_text() {
        println!("{rendered}");
    }
    Ok(())
}

fn test_fixtures(args: TestArgs) -> Result<()> {
    let fixtures = discover_fixtures(&args.fixtures)?;
    let mut failures = Vec::new();
    let mut passed = 0usize;

    for fixture in fixtures {
        match run_fixture(&fixture) {
            Ok(()) => passed += 1,
            Err(message) => failures.push(format!("{}: {message}", fixture.display())),
        }
    }

    println!("passed {passed} fixture(s)");
    if failures.is_empty() {
        return Ok(());
    }

    bail!(failures.join("\n"))
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
    println!("created {}", manifest_path.display());
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

fn default_build_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("module");
    PathBuf::from("target")
        .join("gof")
        .join(format!("{stem}.ssa.json"))
}

fn discover_fixtures(root: &Path) -> Result<Vec<PathBuf>> {
    let mut fixtures = Vec::new();
    if !root.exists() {
        bail!("fixture directory does not exist: {}", root.display());
    }
    collect_gof_files(root, &mut fixtures)?;
    fixtures.sort();
    Ok(fixtures)
}

fn collect_gof_files(root: &Path, fixtures: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_gof_files(&path, fixtures)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("gof") {
            fixtures.push(path);
        }
    }
    Ok(())
}

fn run_fixture(path: &Path) -> Result<()> {
    let source = SourceFile::from_path(path)?;
    let is_fail = path
        .components()
        .any(|component| component.as_os_str() == "fail");
    match compile_source(&source, CompileMode::Executable) {
        Ok(_) if is_fail => bail!("expected fixture to fail"),
        Ok(_) => Ok(()),
        Err(error) if is_fail => {
            let expected = path.with_extension("diag");
            if expected.exists() {
                let expected_contents = fs::read_to_string(&expected)?;
                let expected_codes = expected_contents
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>();
                let actual_codes = error.codes();
                if actual_codes != expected_codes {
                    bail!(
                        "diagnostic mismatch: expected {:?}, got {:?}",
                        expected_codes,
                        actual_codes
                    );
                }
            }
            Ok(())
        }
        Err(error) => Err(render_error(&source, error)),
    }
}

fn render_error(source: &SourceFile, error: Diagnostics) -> anyhow::Error {
    anyhow!(error.render(source))
}
