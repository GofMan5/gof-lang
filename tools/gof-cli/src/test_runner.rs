use anyhow::{Result, anyhow, bail};
use gof_compiler::{
    CompileMode, Diagnostic, Diagnostics, SourceFile, Span, TestExecutionOutcome, TestModuleState,
    TestRuntimeOptions, cleanup_test_module_fixtures_with_output, compile_source,
    normalize_source_path, run_module_with_output, run_test_function_with_output,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::TestArgs;

#[derive(Debug, Clone)]
struct LanguageTestFile {
    source_path: PathBuf,
    snapshot_group: PathBuf,
    test_names: Vec<String>,
    compile_error: Option<Diagnostics>,
    compile_duration_ms: u64,
}

#[derive(Debug)]
struct DiscoveredTargets {
    language_files: Vec<LanguageTestFile>,
    fixture_targets: Vec<PathBuf>,
    package_targets: Vec<PathBuf>,
    doctests: Vec<DocTestCase>,
}

#[derive(Debug, Default)]
struct RunSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    todo: usize,
    listed: usize,
}

impl RunSummary {
    fn executed(&self) -> usize {
        self.passed + self.failed + self.skipped + self.todo
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocTestMode {
    Run,
    NoRun,
    CompileFail,
    RuntimeFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedDoctestFence {
    Start(DocTestMode),
    Ignore,
    NotDoctest,
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

#[derive(Debug, Clone)]
struct DoctestRunReport {
    stdout: String,
    stderr: String,
    diagnostics: Vec<serde_json::Value>,
    failure_message: Option<String>,
}

#[derive(Debug, Default)]
struct JsonTestReport {
    summary: RunSummary,
    failures: Vec<String>,
    events: Vec<serde_json::Value>,
    harness_error: Option<String>,
    stopped_early: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MachineReportFormat {
    Json,
    Junit,
}

pub(crate) const TEST_FAILURE_EXIT_CODE: i32 = 10;
pub(crate) const TEST_HARNESS_FAILURE_EXIT_CODE: i32 = 11;

pub(crate) fn run(args: TestArgs) -> Result<()> {
    if args.json {
        return run_machine_report(args, MachineReportFormat::Json);
    }
    if args.junit {
        return run_machine_report(args, MachineReportFormat::Junit);
    }
    run_human(args)
}

fn run_human(args: TestArgs) -> Result<()> {
    run_human_inner(args).map_err(harness_failure_exit)
}

fn run_human_inner(args: TestArgs) -> Result<()> {
    let discovered = discover_targets(&args)?;
    if discovered_targets_is_empty(&discovered) {
        if args.list {
            return Ok(());
        }
        bail!(
            "no language tests, fixtures, package targets, or doctests matched the provided filters"
        );
    }

    let DiscoveredTargets {
        language_files: filtered_language_files,
        fixture_targets: filtered_fixture_targets,
        package_targets: filtered_package_targets,
        doctests: filtered_doctests,
    } = discovered;

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
                return Err(test_failure_exit("stopped after first failure"));
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
        let mut stop_after_file = false;

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

            if report_language_test_result(
                &id,
                &source,
                result,
                &args,
                &mut summary,
                &mut failures,
            ) {
                stop_after_file = true;
                break;
            }
        }

        let cleanup_id = format!("{}::<cleanup>", display_id_for_path(&file.source_path));
        if report_module_fixture_cleanup_result(
            &cleanup_id,
            cleanup_test_module_fixtures_with_output(&compiled.ast, &mut module_state),
            &args,
            &mut summary,
            &mut failures,
        ) {
            stop_after_file = true;
        }

        if stop_after_file && args.fail_fast {
            return Err(test_failure_exit("stopped after first failure"));
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
                        return Err(test_failure_exit("stopped after first failure"));
                    }
                }
            }
        }

        for path in filtered_fixture_targets {
            match crate::run_fixture(&path, args.update_snapshots) {
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
                        return Err(test_failure_exit("stopped after first failure"));
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
                        return Err(test_failure_exit("stopped after first failure"));
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
        Err(test_failure_exit(format!("failing targets: {}", failures.join(", "))))
    }
}

fn run_machine_report(args: TestArgs, format: MachineReportFormat) -> Result<()> {
    let started = Instant::now();
    let report = collect_machine_report(&args);
    emit_machine_report_and_exit(&args, report, started, format)
}

fn collect_machine_report(args: &TestArgs) -> JsonTestReport {
    let discovered = match discover_targets(args) {
        Ok(discovered) => discovered,
        Err(error) => {
            return JsonTestReport {
                harness_error: Some(error.to_string()),
                ..JsonTestReport::default()
            };
        }
    };

    if discovered_targets_is_empty(&discovered) {
        return JsonTestReport {
            harness_error: if args.list {
                None
            } else {
                Some(
                    "no language tests, fixtures, package targets, or doctests matched the provided filters"
                        .to_string(),
                )
            },
            ..JsonTestReport::default()
        };
    }

    let DiscoveredTargets {
        language_files,
        fixture_targets,
        package_targets,
        doctests,
    } = discovered;
    let mut report = JsonTestReport::default();

    if args.list {
        for file in language_files {
            if file.compile_error.is_some() {
                push_json_listed_event(
                    &mut report,
                    &format!("{}::<compile>", display_id_for_path(&file.source_path)),
                    "language-compile",
                    &file.source_path,
                    None,
                );
                continue;
            }
            for test_name in &file.test_names {
                push_json_listed_event(
                    &mut report,
                    &format!("{}::{test_name}", display_id_for_path(&file.source_path)),
                    "language-test",
                    &file.source_path,
                    None,
                );
            }
        }
        for doctest in doctests {
            push_json_listed_event(
                &mut report,
                &doctest_id(&doctest),
                "doctest",
                &doctest.markdown_path,
                Some(doctest.line),
            );
        }
        for path in fixture_targets {
            push_json_listed_event(
                &mut report,
                &display_id_for_path(&path),
                "fixture",
                &path,
                None,
            );
        }
        for entry_path in package_targets {
            push_json_listed_event(
                &mut report,
                &display_id_for_path(&entry_path),
                "package",
                &entry_path,
                None,
            );
        }

        return report;
    }

    'language: for file in language_files {
        if let Some(error) = &file.compile_error {
            let source = match SourceFile::from_path(&file.source_path) {
                Ok(source) => source,
                Err(source_error) => {
                    report.harness_error = Some(source_error.to_string());
                    break;
                }
            };
            push_json_language_compile_event(
                &mut report,
                &format!("{}::<compile>", display_id_for_path(&file.source_path)),
                &source,
                error,
                file.compile_duration_ms,
            );
            if args.fail_fast {
                report.stopped_early = true;
                break;
            }
            continue;
        }

        let source = match SourceFile::from_path(&file.source_path) {
            Ok(source) => source,
            Err(error) => {
                report.harness_error = Some(error.to_string());
                break;
            }
        };
        if let Err(error) = crate::ensure_package_lockfile(&file.source_path, "test") {
            report.harness_error = Some(error.to_string());
            break;
        }
        let compiled = match compile_source(&source, CompileMode::Library) {
            Ok(compiled) => compiled,
            Err(error) => {
                report.harness_error = Some(crate::render_error(&source, error).to_string());
                break;
            }
        };
        let mut module_state = TestModuleState::default();

        for test_name in &file.test_names {
            let id = format!("{}::{test_name}", display_id_for_path(&file.source_path));
            let started_test = Instant::now();
            let result = match run_test_function_with_output(
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
            ) {
                Ok(result) => result,
                Err(error) => {
                    report.harness_error = Some(crate::render_error(&source, error).to_string());
                    break 'language;
                }
            };
            push_json_language_test_event(
                &mut report,
                &id,
                &source,
                started_test.elapsed().as_millis() as u64,
                result,
            );
            if args.fail_fast && report.failures.last().is_some_and(|failure| failure == &id) {
                report.stopped_early = true;
                break 'language;
            }
        }

        let cleanup_id = format!("{}::<cleanup>", display_id_for_path(&file.source_path));
        let cleanup_started = Instant::now();
        let cleanup_result = cleanup_test_module_fixtures_with_output(&compiled.ast, &mut module_state);
        if cleanup_result.error.is_some() {
            push_json_cleanup_event(
                &mut report,
                &cleanup_id,
                &file.source_path,
                cleanup_started.elapsed().as_millis() as u64,
                cleanup_result,
            );
            if args.fail_fast {
                report.stopped_early = true;
                break;
            }
        }
    }

    if report.harness_error.is_none() && !report.stopped_early {
        for doctest in doctests {
            let id = doctest_id(&doctest);
            let started_doctest = Instant::now();
            let result = run_doctest_report(&doctest);
            push_json_doctest_event(
                &mut report,
                &id,
                &doctest,
                started_doctest.elapsed().as_millis() as u64,
                result,
            );
            if args.fail_fast && report.failures.last().is_some_and(|failure| failure == &id) {
                report.stopped_early = true;
                break;
            }
        }
    }

    if report.harness_error.is_none() && !report.stopped_early {
        for path in fixture_targets {
            let id = display_id_for_path(&path);
            let started_fixture = Instant::now();
            let result = crate::run_fixture_report(&path, args.update_snapshots);
            push_json_fixture_event(
                &mut report,
                &id,
                &path,
                started_fixture.elapsed().as_millis() as u64,
                result,
            );
            if args.fail_fast && report.failures.last().is_some_and(|failure| failure == &id) {
                report.stopped_early = true;
                break;
            }
        }
    }

    if report.harness_error.is_none() && !report.stopped_early {
        for entry_path in package_targets {
            if let Err(error) = crate::ensure_package_lockfile(&entry_path, "test") {
                report.harness_error = Some(error.to_string());
                break;
            }
            let id = display_id_for_path(&entry_path);
            let started_package = Instant::now();
            let result = crate::run_package_test_report(&entry_path);
            push_json_package_event(
                &mut report,
                &id,
                &entry_path,
                started_package.elapsed().as_millis() as u64,
                result,
            );
            if args.fail_fast && report.failures.last().is_some_and(|failure| failure == &id) {
                report.stopped_early = true;
                break;
            }
        }
    }

    report
}

fn emit_machine_report_and_exit(
    args: &TestArgs,
    report: JsonTestReport,
    started: Instant,
    format: MachineReportFormat,
) -> Result<()> {
    let duration_ms = started.elapsed().as_millis() as u64;
    match format {
        MachineReportFormat::Json => emit_json_test_report(args, &report, duration_ms)?,
        MachineReportFormat::Junit => emit_junit_test_report(args, &report, duration_ms)?,
    }
    if report.harness_error.is_some() {
        return Err(silent_harness_failure_exit());
    }
    if !report.failures.is_empty() {
        return Err(silent_test_failure_exit());
    }
    Ok(())
}

fn test_failure_exit(message: impl Into<String>) -> anyhow::Error {
    crate::cli_exit_error(TEST_FAILURE_EXIT_CODE, Some(message.into()))
}

fn silent_test_failure_exit() -> anyhow::Error {
    crate::cli_exit_error(TEST_FAILURE_EXIT_CODE, None)
}

fn silent_harness_failure_exit() -> anyhow::Error {
    crate::cli_exit_error(TEST_HARNESS_FAILURE_EXIT_CODE, None)
}

fn harness_failure_exit(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<crate::CliExit>().is_some() {
        error
    } else {
        crate::cli_exit_error(TEST_HARNESS_FAILURE_EXIT_CODE, Some(format!("{error:#}")))
    }
}

fn emit_json_test_report(args: &TestArgs, report: &JsonTestReport, duration_ms: u64) -> Result<()> {
    let inputs = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths.clone()
    };
    let input_ids = inputs
        .into_iter()
        .map(|path| display_id_for_path(&path))
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": "gof.test.report/v1",
            "ok": report.harness_error.is_none() && report.failures.is_empty(),
            "inputs": input_ids,
            "options": {
                "filter": args.filter.clone(),
                "exact": args.exact,
                "list": args.list,
                "failFast": args.fail_fast,
                "noCapture": args.nocapture,
                "updateSnapshots": args.update_snapshots,
                "docs": args.docs,
                "includeIgnored": args.include_ignored,
                "shuffle": args.shuffle,
                "seed": active_shuffle_seed(args)
            },
            "summary": {
                "executed": report.summary.executed(),
                "passed": report.summary.passed,
                "failed": report.summary.failed,
                "skipped": report.summary.skipped,
                "todo": report.summary.todo,
                "listed": report.summary.listed,
                "durationMs": duration_ms
            },
            "stoppedEarly": report.stopped_early,
            "harnessError": report.harness_error.clone(),
            "failures": report.failures.clone(),
            "events": report.events.clone()
        }))?
    );
    Ok(())
}

fn emit_junit_test_report(args: &TestArgs, report: &JsonTestReport, duration_ms: u64) -> Result<()> {
    let inputs = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths.clone()
    };
    let input_ids = inputs
        .into_iter()
        .map(|path| display_id_for_path(&path))
        .collect::<Vec<_>>();

    let tests = report.summary.executed() + usize::from(report.harness_error.is_some());
    let failures = report.summary.failed;
    let errors = usize::from(report.harness_error.is_some());
    let skipped = report.summary.skipped + report.summary.todo;
    let mut xml = String::new();

    writeln!(&mut xml, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    write!(
        &mut xml,
        "<testsuites name=\"gof\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{}\">",
        tests,
        failures,
        errors,
        skipped,
        junit_seconds(duration_ms)
    )?;
    write!(
        &mut xml,
        "<testsuite name=\"gof\" package=\"gof\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{}\">",
        tests,
        failures,
        errors,
        skipped,
        junit_seconds(duration_ms)
    )?;
    xml.push_str("<properties>");
    write_junit_property(&mut xml, "gof.reporter", "junit")?;
    write_junit_property(&mut xml, "gof.summary.executed", &report.summary.executed().to_string())?;
    write_junit_property(&mut xml, "gof.summary.passed", &report.summary.passed.to_string())?;
    write_junit_property(&mut xml, "gof.summary.failed", &report.summary.failed.to_string())?;
    write_junit_property(&mut xml, "gof.summary.skipped", &report.summary.skipped.to_string())?;
    write_junit_property(&mut xml, "gof.summary.todo", &report.summary.todo.to_string())?;
    write_junit_property(&mut xml, "gof.summary.listed", &report.summary.listed.to_string())?;
    write_junit_property(&mut xml, "gof.stoppedEarly", if report.stopped_early { "true" } else { "false" })?;
    if let Some(filter) = &args.filter {
        write_junit_property(&mut xml, "gof.filter", filter)?;
    }
    write_junit_property(&mut xml, "gof.exact", if args.exact { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.list", if args.list { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.failFast", if args.fail_fast { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.noCapture", if args.nocapture { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.updateSnapshots", if args.update_snapshots { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.docs", if args.docs { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.includeIgnored", if args.include_ignored { "true" } else { "false" })?;
    write_junit_property(&mut xml, "gof.shuffle", if args.shuffle { "true" } else { "false" })?;
    if let Some(seed) = active_shuffle_seed(args) {
        write_junit_property(&mut xml, "gof.seed", &seed.to_string())?;
    }
    for (index, input_id) in input_ids.iter().enumerate() {
        write_junit_property(&mut xml, &format!("gof.input.{}", index + 1), input_id)?;
    }
    if args.list {
        for (index, id) in report
            .events
            .iter()
            .filter_map(|event| event.get("id").and_then(serde_json::Value::as_str))
            .enumerate()
        {
            write_junit_property(&mut xml, &format!("gof.listed.{}", index + 1), id)?;
        }
    }
    xml.push_str("</properties>");

    for event in report.events.iter().filter(|event| event_status(event) != "listed") {
        write_junit_testcase(&mut xml, event)?;
    }

    if let Some(error) = &report.harness_error {
        xml.push_str("<testcase classname=\"gof.harness\" name=\"__harness__\" time=\"0\">");
        write!(
            &mut xml,
            "<error message=\"{}\">{}</error>",
            escape_xml_attribute(error),
            escape_xml_text(error)
        )?;
        xml.push_str("</testcase>");
    }

    xml.push_str("</testsuite></testsuites>");
    println!("{xml}");
    Ok(())
}

fn write_junit_testcase(xml: &mut String, event: &serde_json::Value) -> Result<()> {
    let source_path = event_source_path(event);
    write!(
        xml,
        "<testcase classname=\"{}\" name=\"{}\" file=\"{}\" time=\"{}\"",
        escape_xml_attribute(source_path),
        escape_xml_attribute(event_id(event)),
        escape_xml_attribute(source_path),
        junit_seconds(event_duration_ms(event))
    )?;
    if let Some(line) = event_line(event) {
        write!(xml, " line=\"{}\"", line)?;
    }
    xml.push('>');

    match event_status(event) {
        "failed" => {
            let message = event_message(event).unwrap_or_else(|| event_id(event).to_string());
            let body = junit_failure_body(event);
            write!(
                xml,
                "<failure type=\"{}\" message=\"{}\">{}</failure>",
                escape_xml_attribute(event_kind(event)),
                escape_xml_attribute(&message),
                escape_xml_text(&body)
            )?;
            if !event_stdout(event).is_empty() {
                write!(
                    xml,
                    "<system-out>{}</system-out>",
                    escape_xml_text(event_stdout(event))
                )?;
            }
        }
        "skipped" => {
            let message = event_message(event).unwrap_or_default();
            write!(
                xml,
                "<skipped message=\"{}\" />",
                escape_xml_attribute(&message)
            )?;
            write_junit_case_output(xml, event)?;
        }
        "todo" => {
            let message = event_message(event).unwrap_or_default();
            write!(
                xml,
                "<skipped type=\"todo\" message=\"{}\" />",
                escape_xml_attribute(&message)
            )?;
            write_junit_case_output(xml, event)?;
        }
        _ => write_junit_case_output(xml, event)?,
    }

    xml.push_str("</testcase>");
    Ok(())
}

fn write_junit_case_output(xml: &mut String, event: &serde_json::Value) -> Result<()> {
    if !event_stdout(event).is_empty() {
        write!(
            xml,
            "<system-out>{}</system-out>",
            escape_xml_text(event_stdout(event))
        )?;
    }
    if !event_stderr(event).is_empty() {
        write!(
            xml,
            "<system-err>{}</system-err>",
            escape_xml_text(event_stderr(event))
        )?;
    }
    Ok(())
}

fn write_junit_property(xml: &mut String, name: &str, value: &str) -> Result<()> {
    write!(
        xml,
        "<property name=\"{}\" value=\"{}\" />",
        escape_xml_attribute(name),
        escape_xml_attribute(value)
    )?;
    Ok(())
}

fn junit_failure_body(event: &serde_json::Value) -> String {
    if !event_stderr(event).is_empty() {
        return event_stderr(event).to_string();
    }
    if let Some(message) = event_message(event) {
        return message;
    }
    let diagnostics = event
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|diagnostic| {
            let code = diagnostic.get("code").and_then(serde_json::Value::as_str)?;
            let message = diagnostic.get("message").and_then(serde_json::Value::as_str)?;
            Some(format!("{code}: {message}"))
        })
        .collect::<Vec<_>>();
    if diagnostics.is_empty() {
        event_id(event).to_string()
    } else {
        diagnostics.join("\n")
    }
}

fn event_id(event: &serde_json::Value) -> &str {
    event
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

fn event_kind(event: &serde_json::Value) -> &str {
    event
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

fn event_status(event: &serde_json::Value) -> &str {
    event
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

fn event_source_path(event: &serde_json::Value) -> &str {
    event
        .get("sourcePath")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

fn event_line(event: &serde_json::Value) -> Option<u64> {
    event.get("line").and_then(serde_json::Value::as_u64)
}

fn event_duration_ms(event: &serde_json::Value) -> u64 {
    event
        .get("durationMs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

fn event_stdout(event: &serde_json::Value) -> &str {
    event
        .get("stdout")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn event_stderr(event: &serde_json::Value) -> &str {
    event
        .get("stderr")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn event_message(event: &serde_json::Value) -> Option<String> {
    event
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn junit_seconds(duration_ms: u64) -> String {
    format!("{:.3}", duration_ms as f64 / 1000.0)
}

fn escape_xml_attribute(value: &str) -> String {
    escape_xml_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn discover_targets(args: &TestArgs) -> Result<DiscoveredTargets> {
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
            args.include_ignored,
        )?;
        if args.docs {
            collect_doctest_targets(input, &mut doctest_candidates, args.include_ignored)?;
        }
    }

    let mut language_files = language_candidates
        .into_iter()
        .map(discover_language_test_file)
        .collect::<Result<Vec<_>>>()?;
    language_files.sort_by(|left, right| left.source_path.cmp(&right.source_path));

    let mut discovered = DiscoveredTargets {
        language_files: filter_language_tests(language_files, args),
        fixture_targets: filter_paths(fixture_targets.into_iter().collect(), args, |path| {
            display_id_for_path(path)
        }),
        package_targets: filter_paths(package_targets.into_iter().collect(), args, |path| {
            display_id_for_path(path)
        }),
        doctests: filter_doctests(
            doctest_candidates
                .into_iter()
                .map(discover_doctests_in_markdown)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            args,
        ),
    };
    apply_target_ordering(&mut discovered, args);
    Ok(discovered)
}

fn discovered_targets_is_empty(discovered: &DiscoveredTargets) -> bool {
    discovered.language_files.is_empty()
        && discovered.fixture_targets.is_empty()
        && discovered.package_targets.is_empty()
        && discovered.doctests.is_empty()
}

fn build_json_event(
    id: &str,
    kind: &str,
    status: &str,
    source_path: &Path,
    line: Option<usize>,
    duration_ms: u64,
    stdout: String,
    stderr: String,
    message: Option<String>,
    diagnostics: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "id": id,
        "kind": kind,
        "status": status,
        "sourcePath": display_id_for_path(source_path),
        "line": line,
        "durationMs": duration_ms,
        "stdout": stdout,
        "stderr": stderr,
        "message": message,
        "diagnostics": diagnostics
    })
}

fn push_json_listed_event(
    report: &mut JsonTestReport,
    id: &str,
    kind: &str,
    source_path: &Path,
    line: Option<usize>,
) {
    report.summary.listed += 1;
    report.events.push(build_json_event(
        id,
        kind,
        "listed",
        source_path,
        line,
        0,
        String::new(),
        String::new(),
        None,
        Vec::new(),
    ));
}

fn push_json_language_compile_event(
    report: &mut JsonTestReport,
    id: &str,
    source: &SourceFile,
    diagnostics: &Diagnostics,
    duration_ms: u64,
) {
    report.summary.failed += 1;
    report.failures.push(id.to_string());
    report.events.push(build_json_event(
        id,
        "language-compile",
        "failed",
        source.path(),
        None,
        duration_ms,
        String::new(),
        diagnostics.render(source),
        Some(diagnostics.to_string()),
        crate::diagnostics_to_json(source, diagnostics),
    ));
}

fn push_json_language_test_event(
    report: &mut JsonTestReport,
    id: &str,
    source: &SourceFile,
    duration_ms: u64,
    result: gof_compiler::TestExecutionResult,
) {
    let mut status = "passed";
    let stdout = result.stdout.clone();
    let mut stderr = String::new();
    let mut message = None;
    let mut diagnostics = Vec::new();

    if let Some(cleanup_error) = result.cleanup_error {
        report.summary.failed += 1;
        report.failures.push(id.to_string());
        status = "failed";
        match result.outcome {
            TestExecutionOutcome::Passed => {}
            TestExecutionOutcome::Skipped(skip_message) => {
                stderr.push_str(&format!(
                    "note: test requested skip before teardown: {skip_message}"
                ));
            }
            TestExecutionOutcome::Todo(todo_message) => {
                stderr.push_str(&format!(
                    "note: test was marked todo before teardown: {todo_message}"
                ));
            }
            TestExecutionOutcome::Failed(failure_message) => {
                stderr.push_str(&failure_message);
            }
            TestExecutionOutcome::FailedDiagnostics(failed_diagnostics) => {
                diagnostics = crate::diagnostics_to_json(source, &failed_diagnostics);
                stderr.push_str(&failed_diagnostics.render(source));
            }
        }
        if !stderr.is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(&cleanup_error);
        message = Some(cleanup_error);
    } else {
        match result.outcome {
            TestExecutionOutcome::Passed => {
                report.summary.passed += 1;
            }
            TestExecutionOutcome::Skipped(skip_message) => {
                report.summary.skipped += 1;
                status = "skipped";
                message = Some(skip_message);
            }
            TestExecutionOutcome::Todo(todo_message) => {
                report.summary.todo += 1;
                status = "todo";
                message = Some(todo_message);
            }
            TestExecutionOutcome::Failed(failure_message) => {
                report.summary.failed += 1;
                report.failures.push(id.to_string());
                status = "failed";
                stderr = failure_message.clone();
                message = Some(failure_message);
            }
            TestExecutionOutcome::FailedDiagnostics(failed_diagnostics) => {
                report.summary.failed += 1;
                report.failures.push(id.to_string());
                status = "failed";
                stderr = failed_diagnostics.render(source);
                diagnostics = crate::diagnostics_to_json(source, &failed_diagnostics);
                message = Some(failed_diagnostics.to_string());
            }
        }
    }

    report.events.push(build_json_event(
        id,
        "language-test",
        status,
        source.path(),
        None,
        duration_ms,
        stdout,
        stderr,
        message,
        diagnostics,
    ));
}

fn push_json_cleanup_event(
    report: &mut JsonTestReport,
    id: &str,
    source_path: &Path,
    duration_ms: u64,
    result: gof_compiler::FixtureCleanupResult,
) {
    let Some(error) = result.error else {
        return;
    };
    report.summary.failed += 1;
    report.failures.push(id.to_string());
    report.events.push(build_json_event(
        id,
        "module-cleanup",
        "failed",
        source_path,
        None,
        duration_ms,
        result.stdout,
        error.clone(),
        Some(error),
        Vec::new(),
    ));
}

fn push_json_doctest_event(
    report: &mut JsonTestReport,
    id: &str,
    doctest: &DocTestCase,
    duration_ms: u64,
    result: DoctestRunReport,
) {
    let status = if result.failure_message.is_some() {
        report.summary.failed += 1;
        report.failures.push(id.to_string());
        "failed"
    } else {
        report.summary.passed += 1;
        "passed"
    };
    report.events.push(build_json_event(
        id,
        "doctest",
        status,
        &doctest.markdown_path,
        Some(doctest.line),
        duration_ms,
        result.stdout,
        result.stderr,
        result.failure_message,
        result.diagnostics,
    ));
}

fn push_json_fixture_event(
    report: &mut JsonTestReport,
    id: &str,
    path: &Path,
    duration_ms: u64,
    result: crate::FixtureRunReport,
) {
    let status = if result.failure_message.is_some() {
        report.summary.failed += 1;
        report.failures.push(id.to_string());
        "failed"
    } else {
        report.summary.passed += 1;
        "passed"
    };
    report.events.push(build_json_event(
        id,
        "fixture",
        status,
        path,
        None,
        duration_ms,
        result.stdout,
        result.stderr,
        result.failure_message,
        result.diagnostics,
    ));
}

fn push_json_package_event(
    report: &mut JsonTestReport,
    id: &str,
    entry_path: &Path,
    duration_ms: u64,
    result: crate::PackageTestReport,
) {
    let status = if result.failure_message.is_some() {
        report.summary.failed += 1;
        report.failures.push(id.to_string());
        "failed"
    } else {
        report.summary.passed += 1;
        "passed"
    };
    report.events.push(build_json_event(
        id,
        "package",
        status,
        entry_path,
        None,
        duration_ms,
        result.stdout,
        result.stderr,
        result.failure_message,
        result.diagnostics,
    ));
}

fn run_doctest_report(doctest: &DocTestCase) -> DoctestRunReport {
    let source = SourceFile::new(&doctest.synthetic_source_path, &doctest.source);
    let compile_mode = compile_mode_for_doctest(doctest);
    match doctest.mode {
        DocTestMode::CompileFail => match compile_source(&source, compile_mode) {
            Ok(_) => DoctestRunReport {
                stdout: String::new(),
                stderr: format!(
                    "expected doctest compile failure at {}:{} but compilation succeeded",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line
                ),
                diagnostics: Vec::new(),
                failure_message: Some(format!(
                    "expected doctest compile failure at {}:{} but compilation succeeded",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line
                )),
            },
            Err(error) => DoctestRunReport {
                stdout: String::new(),
                stderr: error.render(&source),
                diagnostics: crate::diagnostics_to_json(&source, &error),
                failure_message: None,
            },
        },
        DocTestMode::NoRun => match compile_source(&source, compile_mode) {
            Ok(_) => DoctestRunReport {
                stdout: String::new(),
                stderr: String::new(),
                diagnostics: Vec::new(),
                failure_message: None,
            },
            Err(error) => {
                let stderr = format!(
                    "doctest compile failed at {}:{}\n{}",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line,
                    error.render(&source)
                );
                DoctestRunReport {
                    stdout: String::new(),
                    stderr,
                    diagnostics: crate::diagnostics_to_json(&source, &error),
                    failure_message: Some(format!(
                        "doctest compile failed at {}:{}",
                        display_id_for_path(&doctest.markdown_path),
                        doctest.line
                    )),
                }
            }
        },
        DocTestMode::Run => match run_module_with_output(&source) {
            Ok(result) => DoctestRunReport {
                stdout: crate::render_execution_stdout(&result),
                stderr: String::new(),
                diagnostics: Vec::new(),
                failure_message: None,
            },
            Err(error) => {
                let stderr = format!(
                    "doctest execution failed at {}:{}\n{}",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line,
                    error.render(&source)
                );
                DoctestRunReport {
                    stdout: String::new(),
                    stderr,
                    diagnostics: crate::diagnostics_to_json(&source, &error),
                    failure_message: Some(format!(
                        "doctest execution failed at {}:{}",
                        display_id_for_path(&doctest.markdown_path),
                        doctest.line
                    )),
                }
            }
        },
        DocTestMode::RuntimeFail => match run_module_with_output(&source) {
            Ok(result) => DoctestRunReport {
                stdout: crate::render_execution_stdout(&result),
                stderr: format!(
                    "expected doctest runtime failure at {}:{} but execution succeeded",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line
                ),
                diagnostics: Vec::new(),
                failure_message: Some(format!(
                    "expected doctest runtime failure at {}:{} but execution succeeded",
                    display_id_for_path(&doctest.markdown_path),
                    doctest.line
                )),
            },
            Err(error) => DoctestRunReport {
                stdout: String::new(),
                stderr: error.render(&source),
                diagnostics: crate::diagnostics_to_json(&source, &error),
                failure_message: None,
            },
        },
    }
}

fn report_language_test_result(
    id: &str,
    source: &SourceFile,
    result: gof_compiler::TestExecutionResult,
    args: &TestArgs,
    summary: &mut RunSummary,
    failures: &mut Vec<String>,
) -> bool {
    let cleanup_failed = result.cleanup_error.is_some();
    let show_output =
        args.nocapture || cleanup_failed || !matches!(result.outcome, TestExecutionOutcome::Passed);
    if show_output && !result.stdout.is_empty() {
        eprintln!("stdout[{id}]:");
        print!("{}", result.stdout);
    }

    if cleanup_failed {
        eprintln!("FAIL {id}");
        match result.outcome {
            TestExecutionOutcome::Passed => {}
            TestExecutionOutcome::Skipped(message) => {
                eprintln!("note: test requested skip before teardown: {message}");
            }
            TestExecutionOutcome::Todo(message) => {
                eprintln!("note: test was marked todo before teardown: {message}");
            }
            TestExecutionOutcome::Failed(message) => {
                eprintln!("{message}");
            }
            TestExecutionOutcome::FailedDiagnostics(diagnostics) => {
                eprintln!("{}", diagnostics.render(source));
            }
        }
        eprintln!(
            "{}",
            result
                .cleanup_error
                .expect("cleanup failure should be present when cleanup_failed is true")
        );
        summary.failed += 1;
        failures.push(id.to_string());
        return args.fail_fast;
    }

    match result.outcome {
        TestExecutionOutcome::Passed => {
            println!("ok {id}");
            summary.passed += 1;
            false
        }
        TestExecutionOutcome::Skipped(message) => {
            println!("skip {id}: {message}");
            summary.skipped += 1;
            false
        }
        TestExecutionOutcome::Todo(message) => {
            println!("todo {id}: {message}");
            summary.todo += 1;
            false
        }
        TestExecutionOutcome::Failed(message) => {
            eprintln!("FAIL {id}");
            eprintln!("{message}");
            summary.failed += 1;
            failures.push(id.to_string());
            args.fail_fast
        }
        TestExecutionOutcome::FailedDiagnostics(diagnostics) => {
            eprintln!("FAIL {id}");
            eprintln!("{}", diagnostics.render(source));
            summary.failed += 1;
            failures.push(id.to_string());
            args.fail_fast
        }
    }
}

fn report_module_fixture_cleanup_result(
    id: &str,
    result: gof_compiler::FixtureCleanupResult,
    args: &TestArgs,
    summary: &mut RunSummary,
    failures: &mut Vec<String>,
) -> bool {
    let show_output = args.nocapture || result.error.is_some();
    if show_output && !result.stdout.is_empty() {
        eprintln!("stdout[{id}]:");
        print!("{}", result.stdout);
    }

    let Some(error) = result.error else {
        return false;
    };

    eprintln!("FAIL {id}");
    eprintln!("{error}");
    summary.failed += 1;
    failures.push(id.to_string());
    args.fail_fast
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
    let started = Instant::now();
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
                compile_duration_ms: started.elapsed().as_millis() as u64,
            })
        }
        Err(error) => Ok(LanguageTestFile {
            source_path: normalize_source_path(&path),
            snapshot_group,
            test_names: vec!["<compile>".to_string()],
            compile_error: Some(error),
            compile_duration_ms: started.elapsed().as_millis() as u64,
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

fn apply_target_ordering(discovered: &mut DiscoveredTargets, args: &TestArgs) {
    let Some(seed) = active_shuffle_seed(args) else {
        return;
    };

    discovered.language_files.sort_by_cached_key(|file| {
        let id = language_file_order_id(file);
        (shuffle_order_key(seed, &id), id)
    });
    for file in &mut discovered.language_files {
        let path_id = display_id_for_path(&file.source_path);
        file.test_names.sort_by_cached_key(|test_name| {
            let id = format!("{path_id}::{test_name}");
            (shuffle_order_key(seed, &id), id)
        });
    }
    discovered.fixture_targets.sort_by_cached_key(|path| {
        let id = display_id_for_path(path);
        (shuffle_order_key(seed, &id), id)
    });
    discovered.package_targets.sort_by_cached_key(|path| {
        let id = display_id_for_path(path);
        (shuffle_order_key(seed, &id), id)
    });
    discovered.doctests.sort_by_cached_key(|doctest| {
        let id = doctest_id(doctest);
        (shuffle_order_key(seed, &id), id)
    });
}

fn active_shuffle_seed(args: &TestArgs) -> Option<u64> {
    args.shuffle.then_some(args.seed.unwrap_or(0))
}

fn language_file_order_id(file: &LanguageTestFile) -> String {
    let path_id = display_id_for_path(&file.source_path);
    if file.compile_error.is_some() {
        format!("{path_id}::<compile>")
    } else {
        path_id
    }
}

fn shuffle_order_key(seed: u64, value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in seed.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn collect_targets(
    input: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
    package_targets: &mut BTreeSet<PathBuf>,
    include_ignored: bool,
) -> Result<()> {
    if input.is_file() {
        collect_file_target(
            input,
            language_candidates,
            fixture_targets,
            package_targets,
            include_ignored,
        )?;
        return Ok(());
    }

    if input.is_dir() {
        let before_language = language_candidates.len();
        let before_fixtures = fixture_targets.len();
        collect_dir_targets(input, language_candidates, fixture_targets, include_ignored)?;
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

fn collect_doctest_targets(
    input: &Path,
    doctest_candidates: &mut BTreeSet<PathBuf>,
    include_ignored: bool,
) -> Result<()> {
    if input.is_file() {
        if is_markdown_file(input) {
            doctest_candidates.insert(normalize_source_path(input));
        }
        return Ok(());
    }

    if input.is_dir() {
        if collect_repository_doctest_targets(input, doctest_candidates, include_ignored)? {
            return Ok(());
        }
        collect_doctest_dir_targets(input, doctest_candidates, include_ignored)?;
        return Ok(());
    }

    bail!("doctest target does not exist: {}", input.display());
}

fn collect_repository_doctest_targets(
    root: &Path,
    doctest_candidates: &mut BTreeSet<PathBuf>,
    include_ignored: bool,
) -> Result<bool> {
    let readme_path = root.join("README.md");
    let book_src = root.join("docs").join("book").join("src");
    let book_ru_src = root.join("docs").join("book-ru").join("src");
    let has_book_docs = book_src.is_dir() || book_ru_src.is_dir();
    if !has_book_docs {
        return Ok(false);
    }

    if readme_path.is_file() {
        doctest_candidates.insert(normalize_source_path(&readme_path));
    }
    if book_src.is_dir() {
        collect_doctest_dir_targets(&book_src, doctest_candidates, include_ignored)?;
    }
    if book_ru_src.is_dir() {
        collect_doctest_dir_targets(&book_ru_src, doctest_candidates, include_ignored)?;
    }

    Ok(true)
}

fn collect_dir_targets(
    root: &Path,
    language_candidates: &mut BTreeSet<PathBuf>,
    fixture_targets: &mut BTreeSet<PathBuf>,
    include_ignored: bool,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_discovery_dir(&path, include_ignored) {
                continue;
            }
            collect_dir_targets(&path, language_candidates, fixture_targets, include_ignored)?;
        } else {
            collect_candidate_file(&path, language_candidates, fixture_targets, include_ignored);
        }
    }
    Ok(())
}

fn collect_doctest_dir_targets(
    root: &Path,
    doctest_candidates: &mut BTreeSet<PathBuf>,
    include_ignored: bool,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if should_skip_doctest_dir(&path, include_ignored) {
                continue;
            }
            collect_doctest_dir_targets(&path, doctest_candidates, include_ignored)?;
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
    include_ignored: bool,
) -> Result<()> {
    let normalized = normalize_source_path(path);
    if is_fixture_file(path) {
        fixture_targets.insert(normalized);
    } else if is_language_test_candidate(path, include_ignored) {
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
    include_ignored: bool,
) {
    if !is_gof_file(path) {
        return;
    }
    let normalized = normalize_source_path(path);
    if is_fixture_file(path) {
        fixture_targets.insert(normalized);
    } else if is_language_test_candidate(path, include_ignored) {
        language_candidates.insert(normalized);
    }
}

fn is_language_test_candidate(path: &Path, include_ignored: bool) -> bool {
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
        let Some(name) = component.as_os_str().to_str() else {
            return false;
        };
        is_hard_language_test_exclusion(name)
            || (!include_ignored && is_ignored_language_test_component(name))
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

fn should_skip_discovery_dir(path: &Path, include_ignored: bool) -> bool {
    if include_ignored {
        return false;
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(is_ignored_discovery_dir)
}

fn should_skip_doctest_dir(path: &Path, include_ignored: bool) -> bool {
    if include_ignored {
        return false;
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(is_ignored_doctest_dir)
}

fn is_hard_language_test_exclusion(name: &str) -> bool {
    matches!(name, "ui" | "runtime" | "runtime-fail" | "fixtures")
}

fn is_ignored_language_test_component(name: &str) -> bool {
    matches!(name, "support" | "snapshots" | "fuzz" | "stress" | "crashes" | "corpus")
}

fn is_ignored_discovery_dir(name: &str) -> bool {
    matches!(name, "support" | "snapshots" | "fuzz" | "crashes" | "corpus")
}

fn is_ignored_doctest_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules" | "dist-vscode-publish")
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
    let source = SourceFile::new(&path, &text);
    let normalized_path = normalize_source_path(&path);
    let mut doctests = Vec::new();
    let mut inside = false;
    let mut active_mode = None;
    let mut block_lines = Vec::new();
    let mut block_start_line = 0usize;
    let mut active_fence_line = 0usize;
    let mut active_fence_text = String::new();
    let mut ordinal = 0usize;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = raw_line.trim_start();

        if !inside {
            match parse_doctest_fence(trimmed, line_number, raw_line, &source)? {
                ParsedDoctestFence::Start(mode) => {
                    inside = true;
                    active_mode = Some(mode);
                    block_lines.clear();
                    block_start_line = line_number + 1;
                    active_fence_line = line_number;
                    active_fence_text = raw_line.to_string();
                }
                ParsedDoctestFence::Ignore | ParsedDoctestFence::NotDoctest => {}
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
                    markdown_path: normalized_path.clone(),
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

    if inside {
        return Err(doctest_fence_error(
            &source,
            active_fence_line,
            &active_fence_text,
            "GOF3128",
            "unterminated doctest fence",
            "this doctest block starts here but never closes with a matching ``` fence",
            "close the doctest block with ``` or remove the doctest marker",
        ));
    }

    Ok(doctests)
}

fn parse_doctest_fence(
    line: &str,
    line_number: usize,
    raw_line: &str,
    source: &SourceFile,
) -> Result<ParsedDoctestFence> {
    if !line.starts_with("```") {
        return Ok(ParsedDoctestFence::NotDoctest);
    }
    let info = line.trim_start_matches('`').trim();
    let tokens = info.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() || tokens[0] != "gof" {
        return Ok(ParsedDoctestFence::NotDoctest);
    }
    if !tokens.iter().any(|token| *token == "doctest") {
        return Ok(ParsedDoctestFence::NotDoctest);
    }
    if tokens
        .iter()
        .any(|token| *token == "ignore" || *token == "text")
    {
        return Ok(ParsedDoctestFence::Ignore);
    }

    let mut no_run = false;
    let mut compile_fail = false;
    let mut runtime_fail = false;

    for token in tokens.iter().skip(1) {
        match *token {
            "doctest" => {}
            "no_run" => no_run = true,
            "compile_fail" => compile_fail = true,
            "runtime_fail" => runtime_fail = true,
            other => {
                return Err(doctest_fence_error(
                    source,
                    line_number,
                    raw_line,
                    "GOF3126",
                    format!("unknown doctest fence modifier `{other}`"),
                    "allowed doctest modifiers are `no_run`, `compile_fail`, `runtime_fail`, `ignore`, and `text`",
                    "use `gof doctest`, `gof doctest no_run`, `gof doctest compile_fail`, or `gof doctest runtime_fail`",
                ));
            }
        }
    }

    let mode_count = usize::from(no_run) + usize::from(compile_fail) + usize::from(runtime_fail);
    if mode_count > 1 {
        return Err(doctest_fence_error(
            source,
            line_number,
            raw_line,
            "GOF3127",
            "conflicting doctest fence modes",
            "choose at most one execution modifier from `no_run`, `compile_fail`, and `runtime_fail`",
            "keep only one doctest execution modifier on this fence",
        ));
    }

    if compile_fail {
        return Ok(ParsedDoctestFence::Start(DocTestMode::CompileFail));
    }
    if runtime_fail {
        return Ok(ParsedDoctestFence::Start(DocTestMode::RuntimeFail));
    }
    if no_run {
        return Ok(ParsedDoctestFence::Start(DocTestMode::NoRun));
    }

    Ok(ParsedDoctestFence::Start(DocTestMode::Run))
}

fn doctest_fence_error(
    source: &SourceFile,
    line_number: usize,
    raw_line: &str,
    code: &'static str,
    message: impl Into<String>,
    note: impl Into<String>,
    fix_it: &str,
) -> anyhow::Error {
    let diagnostic = Diagnostic::error(
        code,
        message,
        note,
        markdown_line_span(line_number, raw_line),
    )
    .with_fix_it(fix_it)
    .with_source_path(source.path().to_path_buf());
    anyhow!(Diagnostics(vec![diagnostic]).render(source))
}

fn markdown_line_span(line_number: usize, raw_line: &str) -> Span {
    Span::new(line_number, 1, raw_line.chars().count().max(1) + 1)
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
    let report = run_doctest_report(doctest);
    if let Some(error) = report.failure_message {
        bail!(error);
    }
    Ok(())
}

fn compile_mode_for_doctest(doctest: &DocTestCase) -> CompileMode {
    if doctest.source.contains("fn main(") {
        CompileMode::Executable
    } else {
        CompileMode::Library
    }
}
