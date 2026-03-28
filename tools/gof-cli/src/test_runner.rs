use anyhow::{Result, anyhow, bail};
use gof_compiler::{
    CompileMode, Diagnostics, SourceFile, TestExecutionOutcome, TestModuleState,
    TestRuntimeOptions, compile_source, normalize_source_path, run_module_with_output,
    run_test_function_with_output,
};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::TestArgs;

#[derive(Debug, Clone)]
struct LanguageTestFile {
    source_path: PathBuf,
    snapshot_group: PathBuf,
    test_names: Vec<String>,
    compile_error: Option<Diagnostics>,
}

#[derive(Debug, Default)]
struct RunSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    todo: usize,
    listed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocTestMode {
    Run,
    NoRun,
    CompileFail,
    RuntimeFail,
}

#[derive(Debug, Clone)]
struct DocTestCase {
    markdown_path: PathBuf,
    synthetic_source_path: PathBuf,
    line: usize,
    ordinal: usize,
    mode: DocTestMode,
    source: String,
}

pub(crate) fn run(args: TestArgs) -> Result<()> {
    let inputs = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths.clone()
    };

    let mut language_candidates = BTreeSet::new();
    let mut fixture_targets = BTreeSet::new();
    let mut package_targets = BTreeSet::new();
    let mut doctest_candidates = BTreeSet::new();

    for input in &inputs {
        collect_targets(
            input,
            &mut language_candidates,
            &mut fixture_targets,
            &mut package_targets,
        )?;
        if args.docs {
            collect_doctest_targets(input, &mut doctest_candidates)?;
        }
    }

    let mut language_files = language_candidates
        .into_iter()
        .map(discover_language_test_file)
        .collect::<Result<Vec<_>>>()?;
    language_files.sort_by(|left, right| left.source_path.cmp(&right.source_path));

    let filtered_language_files = filter_language_tests(language_files, &args);
    let filtered_fixture_targets =
        filter_paths(fixture_targets.into_iter().collect(), &args, |path| {
            display_id_for_path(path)
        });
    let filtered_package_targets =
        filter_paths(package_targets.into_iter().collect(), &args, |path| {
            display_id_for_path(path)
        });
    let filtered_doctests = filter_doctests(
        doctest_candidates
            .into_iter()
            .map(discover_doctests_in_markdown)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect(),
        &args,
    );

    if filtered_language_files.is_empty()
        && filtered_fixture_targets.is_empty()
        && filtered_package_targets.is_empty()
        && filtered_doctests.is_empty()
    {
        if args.list {
            return Ok(());
        }
        bail!(
            "no language tests, fixtures, package targets, or doctests matched the provided filters"
        );
    }

    let mut summary = RunSummary::default();
    let mut failures = Vec::new();

    for file in filtered_language_files {
        if let Some(error) = &file.compile_error {
            let source = SourceFile::from_path(&file.source_path)?;
            let id = format!("{}::<compile>", display_id_for_path(&file.source_path));
            if args.list {
                println!("{id}");
                summary.listed += 1;
                continue;
            }
            eprintln!("FAIL {id}");
            eprintln!("{}", error.render(&source));
            summary.failed += 1;
            failures.push(id);
            if args.fail_fast {
                bail!("stopped after first failure");
            }
            continue;
        }

        if args.list {
            for test_name in &file.test_names {
                println!("{}::{test_name}", display_id_for_path(&file.source_path));
                summary.listed += 1;
            }
            continue;
        }

        let source = SourceFile::from_path(&file.source_path)?;
        crate::ensure_package_lockfile(&file.source_path, "test")?;
        let compiled = compile_source(&source, CompileMode::Library)
            .map_err(|error| crate::render_error(&source, error))?;
        let mut module_state = TestModuleState::default();

        for test_name in &file.test_names {
            let id = format!("{}::{test_name}", display_id_for_path(&file.source_path));
            let result = run_test_function_with_output(
                &compiled.ast,
                &compiled.typed_hir,
                test_name,
                TestRuntimeOptions {
                    snapshot_root: snapshot_root_for_source(&file.source_path),
                    snapshot_group: file.snapshot_group.clone(),
                    update_snapshots: args.update_snapshots,
                    program_args: Vec::new(),
                },
                &mut module_state,
            )
            .map_err(|error| crate::render_error(&source, error))?;

            let show_output =
                args.nocapture || !matches!(result.outcome, TestExecutionOutcome::Passed);
            if show_output && !result.stdout.is_empty() {
                eprintln!("stdout[{id}]:");
                print!("{}", result.stdout);
            }

            match result.outcome {
                TestExecutionOutcome::Passed => {
                    println!("ok {id}");
                    summary.passed += 1;
                }
                TestExecutionOutcome::Skipped(message) => {
                    println!("skip {id}: {message}");
                    summary.skipped += 1;
                }
                TestExecutionOutcome::Todo(message) => {
                    println!("todo {id}: {message}");
                    summary.todo += 1;
                }
                TestExecutionOutcome::Failed(message) => {
                    eprintln!("FAIL {id}");
                    eprintln!("{message}");
                    summary.failed += 1;
                    failures.push(id);
                    if args.fail_fast {
                        bail!("stopped after first failure");
                    }
                }
                TestExecutionOutcome::FailedDiagnostics(diagnostics) => {
                    eprintln!("FAIL {id}");
                    eprintln!("{}", diagnostics.render(&source));
                    summary.failed += 1;
                    failures.push(id);
                    if args.fail_fast {
                        bail!("stopped after first failure");
                    }
                }
            }
        }
    }

    if !args.list {
        for doctest in filtered_doctests {
            let id = doctest_id(&doctest);
            match run_doctest(&doctest) {
                Ok(()) => {
                    println!("ok {id}");
                    summary.passed += 1;
                }
                Err(error) => {
                    eprintln!("FAIL {id}");
                    eprintln!("{error:#}");
                    summary.failed += 1;
                    failures.push(id);
                    if args.fail_fast {
                        bail!("stopped after first failure");
                    }
                }
            }
        }

        for path in filtered_fixture_targets {
            match crate::run_fixture(&path) {
                Ok(()) => {
                    println!("ok {}", display_id_for_path(&path));
                    summary.passed += 1;
                }
                Err(error) => {
                    eprintln!("FAIL {}", display_id_for_path(&path));
                    eprintln!("{error:#}");
                    summary.failed += 1;
                    failures.push(display_id_for_path(&path));
                    if args.fail_fast {
                        bail!("stopped after first failure");
                    }
                }
            }
        }

        for entry_path in filtered_package_targets {
            crate::ensure_package_lockfile(&entry_path, "test")?;
            match crate::run_package_test(&entry_path) {
                Ok(()) => {
                    println!("ok {}", display_id_for_path(&entry_path));
                    summary.passed += 1;
                }
                Err(error) => {
                    eprintln!("FAIL {}", display_id_for_path(&entry_path));
                    eprintln!("{error:#}");
                    summary.failed += 1;
                    failures.push(display_id_for_path(&entry_path));
                    if args.fail_fast {
                        bail!("stopped after first failure");
                    }
                }
            }
        }
    } else {
        for doctest in filtered_doctests {
            println!("{}", doctest_id(&doctest));
            summary.listed += 1;
        }
    }

    if args.list {
        println!("listed {}", summary.listed);
        return Ok(());
    }

    println!(
        "test result: {} passed; {} failed; {} skipped; {} todo",
        summary.passed, summary.failed, summary.skipped, summary.todo
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("failing targets: {}", failures.join(", ")))
    }
}

fn filter_doctests(doctests: Vec<DocTestCase>, args: &TestArgs) -> Vec<DocTestCase> {
    doctests
        .into_iter()
        .filter(|doctest| {
            args.filter.as_ref().is_none_or(|filter| {
                let id = doctest_id(doctest);
                matches_filter(&id, filter, args.exact)
            })
        })
        .collect()
}

fn discover_language_test_file(path: PathBuf) -> Result<LanguageTestFile> {
    crate::ensure_package_lockfile(&path, "test")?;
    let source = SourceFile::from_path(&path)?;
    let snapshot_group = snapshot_group_for_source(&path);
    match compile_source(&source, CompileMode::Library) {
        Ok(compiled) => {
            let mut test_names = compiled
                .ast
                .functions
                .iter()
                .filter(|function| {
                    function.receiver_type.is_none()
                        && matches!(function.kind, gof_compiler::ast::FunctionKind::Test)
                })
                .map(|function| function.name.clone())
                .collect::<Vec<_>>();
            test_names.sort();
            Ok(LanguageTestFile {
                source_path: normalize_source_path(&path),
                snapshot_group,
                test_names,
                compile_error: None,
            })
        }
        Err(error) => Ok(LanguageTestFile {
            source_path: normalize_source_path(&path),
            snapshot_group,
            test_names: vec!["<compile>".to_string()],
            compile_error: Some(error),
        }),
    }
}

fn filter_language_tests(files: Vec<LanguageTestFile>, args: &TestArgs) -> Vec<LanguageTestFile> {
    files
        .into_iter()
        .filter_map(|mut file| {
            if let Some(filter) = &args.filter {
                let path_id = display_id_for_path(&file.source_path);
                if file.compile_error.is_some() {
                    if matches_filter(&format!("{path_id}::<compile>"), filter, args.exact) {
                        return Some(file);
                    }
                    return None;
                }

                file.test_names.retain(|test_name| {
                    matches_filter(&format!("{path_id}::{test_name}"), filter, args.exact)
                        || matches_filter(test_name, filter, args.exact)
                });
            }

            if file.compile_error.is_some() || !file.test_names.is_empty() {
                Some(file)
            } else {
                None
            }
        })
        .collect()
}

fn filter_paths<F>(paths: Vec<PathBuf>, args: &TestArgs, id_fn: F) -> Vec<PathBuf>
where
    F: Fn(&Path) -> String,
{
    paths
        .into_iter()
        .filter(|path| {
            args.filter.as_ref().is_none_or(|filter| {
                let id = id_fn(path);
                matches_filter(&id, filter, args.exact)
            })
        })
        .collect()
}

fn matches_filter(value: &str, filter: &str, exact: bool) -> bool {
    if exact {
        value == filter
    } else {
        value.contains(filter)
    }
}

fn collect_targets(
    input: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
    package_targets: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    if input.is_file() {
        collect_file_target(input, language_candidates, fixture_targets, package_targets)?;
        return Ok(());
    }

    if input.is_dir() {
        let before_language = language_candidates.len();
        let before_fixtures = fixture_targets.len();
        collect_dir_targets(input, language_candidates, fixture_targets)?;
        if language_candidates.len() == before_language && fixture_targets.len() == before_fixtures
        {
            if let Some(entry_path) = crate::resolve_package_test_input(input)? {
                package_targets.insert(normalize_source_path(&entry_path));
            }
        }
        return Ok(());
    }

    bail!("test target does not exist: {}", input.display());
}

fn collect_doctest_targets(input: &Path, doctest_candidates: &mut BTreeSet<PathBuf>) -> Result<()> {
    if input.is_file() {
        if is_markdown_file(input) {
            doctest_candidates.insert(normalize_source_path(input));
        }
        return Ok(());
    }

    if input.is_dir() {
        collect_doctest_dir_targets(input, doctest_candidates)?;
        return Ok(());
    }

    bail!("doctest target does not exist: {}", input.display());
}

fn collect_dir_targets(
    root: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_discovery_dir(&path) {
                continue;
            }
            collect_dir_targets(&path, language_candidates, fixture_targets)?;
        } else {
            collect_candidate_file(&path, language_candidates, fixture_targets);
        }
    }
    Ok(())
}

fn collect_doctest_dir_targets(
    root: &Path,
    doctest_candidates: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_doctest_dir(&path) {
                continue;
            }
            collect_doctest_dir_targets(&path, doctest_candidates)?;
        } else if is_markdown_file(&path) {
            doctest_candidates.insert(normalize_source_path(&path));
        }
    }
    Ok(())
}

fn collect_file_target(
    path: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
    package_targets: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let normalized = normalize_source_path(path);
    if is_fixture_file(path) {
        fixture_targets.insert(normalized);
    } else if is_language_test_candidate(path) {
        language_candidates.insert(normalized);
    } else if let Some(entry_path) = crate::resolve_package_test_input(path)? {
        package_targets.insert(normalize_source_path(&entry_path));
    }
    Ok(())
}

fn collect_candidate_file(
    path: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
) {
    if !is_gof_file(path) {
        return;
    }
    let normalized = normalize_source_path(path);
    if is_fixture_file(path) {
        fixture_targets.insert(normalized);
    } else if is_language_test_candidate(path) {
        language_candidates.insert(normalized);
    }
}

fn is_language_test_candidate(path: &Path) -> bool {
    if !is_gof_file(path) {
        return false;
    }

    if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with("_test.gof"))
    {
        return true;
    }

    let components = path.components().collect::<Vec<_>>();
    let in_tests = components
        .iter()
        .any(|component| component.as_os_str() == OsStr::new("tests"));
    let excluded = components.iter().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("support" | "snapshots" | "ui" | "runtime" | "fixtures" | "fuzz" | "stress")
        )
    });
    in_tests && !excluded
}

fn is_fixture_file(path: &Path) -> bool {
    if !is_gof_file(path) {
        return false;
    }
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("fixtures" | "ui" | "runtime-fail" | "runtime")
        )
    })
}

fn is_gof_file(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("gof")
}

fn is_markdown_file(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("md")
}

fn should_skip_discovery_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                "support" | "snapshots" | "fuzz" | "crashes" | "corpus"
            )
        })
}

fn should_skip_doctest_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git" | "target" | "node_modules" | "dist-vscode-publish"
            )
        })
}

fn display_id_for_path(path: &Path) -> String {
    let normalized = normalize_source_path(path);
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match normalized.strip_prefix(&current_dir) {
        Ok(relative) => crate::display_path(relative),
        Err(_) => crate::display_path(&normalized),
    }
    .replace('\\', "/")
}

fn snapshot_group_for_source(path: &Path) -> PathBuf {
    let normalized = normalize_source_path(path);
    let project_root = project_root_for_source(&normalized);
    let mut group = normalized
        .strip_prefix(&project_root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| {
            PathBuf::from(
                normalized
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("test"),
            )
        });
    group.set_extension("");
    group
}

fn snapshot_root_for_source(path: &Path) -> PathBuf {
    project_root_for_source(path)
        .join("tests")
        .join("snapshots")
}

fn project_root_for_source(path: &Path) -> PathBuf {
    let normalized = normalize_source_path(path);
    for ancestor in normalized.ancestors() {
        if ancestor.file_name().and_then(|value| value.to_str()) == Some("tests") {
            return ancestor
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
        }
        if ancestor.file_name().and_then(|value| value.to_str()) == Some("src") {
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

fn discover_doctests_in_markdown(path: PathBuf) -> Result<Vec<DocTestCase>> {
    let text = fs::read_to_string(&path)?;
    let mut doctests = Vec::new();
    let mut inside = false;
    let mut active_mode = None;
    let mut block_lines = Vec::new();
    let mut block_start_line = 0usize;
    let mut ordinal = 0usize;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim_start();

        if !inside {
            if let Some(mode) = parse_doctest_fence(trimmed) {
                inside = true;
                active_mode = Some(mode);
                block_lines.clear();
                block_start_line = line_number + 1;
            }
            continue;
        }

        if trimmed.starts_with("```") {
            if let Some(mode) = active_mode.take() {
                ordinal += 1;
                let synthetic_name = format!(
                    "{}.doctest-{}.gof",
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("doc"),
                    ordinal
                );
                let synthetic_source_path = path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(synthetic_name);
                doctests.push(DocTestCase {
                    markdown_path: normalize_source_path(&path),
                    synthetic_source_path: normalize_source_path(&synthetic_source_path),
                    line: block_start_line,
                    ordinal,
                    mode,
                    source: block_lines.join("\n"),
                });
            }
            inside = false;
            continue;
        }

        block_lines.push(raw_line.to_string());
    }

    Ok(doctests)
}

fn parse_doctest_fence(line: &str) -> Option<DocTestMode> {
    if !line.starts_with("```") {
        return None;
    }
    let info = line.trim_start_matches('`').trim();
    let tokens = info.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() || tokens[0] != "gof" {
        return None;
    }
    if !tokens.iter().any(|token| *token == "doctest") {
        return None;
    }
    if tokens
        .iter()
        .any(|token| *token == "ignore" || *token == "text")
    {
        return None;
    }
    if tokens.iter().any(|token| *token == "compile_fail") {
        return Some(DocTestMode::CompileFail);
    }
    if tokens.iter().any(|token| *token == "runtime_fail") {
        return Some(DocTestMode::RuntimeFail);
    }
    if tokens.iter().any(|token| *token == "no_run") {
        return Some(DocTestMode::NoRun);
    }
    Some(DocTestMode::Run)
}

fn doctest_id(doctest: &DocTestCase) -> String {
    let mode = match doctest.mode {
        DocTestMode::Run => "doctest",
        DocTestMode::NoRun => "doctest-no-run",
        DocTestMode::CompileFail => "doctest-compile-fail",
        DocTestMode::RuntimeFail => "doctest-runtime-fail",
    };
    format!(
        "{}:{}::{}#{}",
        display_id_for_path(&doctest.markdown_path),
        doctest.line,
        mode,
        doctest.ordinal
    )
}

fn run_doctest(doctest: &DocTestCase) -> Result<()> {
    let source = SourceFile::new(&doctest.synthetic_source_path, &doctest.source);
    let compile_mode = compile_mode_for_doctest(doctest);
    match doctest.mode {
        DocTestMode::CompileFail => match compile_source(&source, compile_mode) {
            Ok(_) => bail!(
                "expected doctest compile failure at {}:{} but compilation succeeded",
                display_id_for_path(&doctest.markdown_path),
                doctest.line
            ),
            Err(_) => Ok(()),
        },
        DocTestMode::NoRun => compile_source(&source, compile_mode)
            .map(|_| ())
            .map_err(|error| {
                anyhow!(
                    "doctest compile failed at {}:{}\n{}",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line,
                    error.render(&source)
                )
            }),
        DocTestMode::Run => run_module_with_output(&source)
            .map(|_| ())
            .map_err(|error| {
                anyhow!(
                    "doctest execution failed at {}:{}\n{}",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line,
                    error.render(&source)
                )
            }),
        DocTestMode::RuntimeFail => match run_module_with_output(&source) {
            Ok(_) => bail!(
                "expected doctest runtime failure at {}:{} but execution succeeded",
                display_id_for_path(&doctest.markdown_path),
                doctest.line
            ),
            Err(_) => Ok(()),
        },
    }
}

fn compile_mode_for_doctest(doctest: &DocTestCase) -> CompileMode {
    if doctest.source.contains("fn main(") {
        CompileMode::Executable
    } else {
        CompileMode::Library
    }
}
