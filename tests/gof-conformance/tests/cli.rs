use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as ProcessCommand;
use tempfile::tempdir;

fn gof_command() -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(gof_conformance::workspace_root())
        .args(["run", "-q", "-p", "gof-cli", "--bin", "gof", "--"]);
    command
}

#[test]
fn gof_run_executes_bootstrap_main() {
    let fixture = gof_conformance::workspace_root()
        .join("tests")
        .join("fixtures")
        .join("pass")
        .join("hello.gof");

    gof_command()
        .arg("run")
        .arg(fixture)
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

#[test]
fn gof_run_executes_calculator_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("calculator.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("107"));
}

#[test]
fn gof_run_executes_factorial_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("factorial.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("120"));
}

#[test]
fn gof_run_executes_hello_print_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("hello_print.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("gof ready"))
        .stdout(predicate::str::contains("42"))
        .stdout(predicate::str::contains("7"));
}

#[test]
fn gof_run_executes_geometry_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("geometry.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

#[test]
fn gof_run_executes_geometry_methods_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("geometry_methods.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("10"));
}

#[test]
fn gof_run_executes_health_gate_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("health_gate.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));
}

#[test]
fn gof_run_executes_list_sum_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("list_sum.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("108"));
}

#[test]
fn gof_run_executes_stdlib_helpers_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("stdlib_helpers.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("18"));
}

#[test]
fn gof_run_executes_dict_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("dict_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("11"));
}

#[test]
fn gof_run_executes_channel_select_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("channel_select.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("9"));
}

#[test]
fn gof_run_executes_io_roundtrip_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("io_roundtrip.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("6"));
}

#[test]
fn gof_run_executes_status_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("status_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("200"));
}

#[test]
fn gof_run_executes_status_match_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("status_match.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("50"));
}

#[test]
fn gof_run_executes_string_metrics_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("string_metrics.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("10"));
}

#[test]
fn gof_run_executes_parallel_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("parallel_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("104"));
}

#[test]
fn gof_run_executes_concurrent_squares_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("concurrent_squares.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("225"));
}

#[test]
fn gof_run_executes_portfolio_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("portfolio")
        .join("main.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("160"));
}

#[test]
fn gof_run_executes_records_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("records")
        .join("main.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("140"));
}

#[test]
fn gof_run_executes_modular_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("modules")
        .join("main.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("121"));
}

#[test]
fn gof_run_executes_local_import_graph() {
    let temp = tempdir().expect("tempdir should exist");
    let helper_path = temp.path().join("math.gof");
    let main_path = temp.path().join("main.gof");

    fs::write(
        &helper_path,
        "fn square(x: int) -> int:\n    return x * x\n",
    )
    .expect("helper module should be written");
    fs::write(
        &main_path,
        "import math\n\nfn main() -> int:\n    return square(9)\n",
    )
    .expect("main module should be written");

    gof_command()
        .arg("run")
        .arg(&main_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("81"));
}

#[test]
fn gof_run_executes_imported_receiver_methods() {
    let temp = tempdir().expect("tempdir should exist");
    let helper_path = temp.path().join("geometry.gof");
    let main_path = temp.path().join("main.gof");

    fs::write(
        &helper_path,
        "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n",
    )
    .expect("helper module should be written");
    fs::write(
        &main_path,
        "import geometry\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n",
    )
    .expect("main module should be written");

    gof_command()
        .arg("run")
        .arg(&main_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("12"));
}

#[test]
fn gof_run_reports_imported_file_diagnostics_with_their_path() {
    let temp = tempdir().expect("tempdir should exist");
    let helper_path = temp.path().join("math.gof");
    let main_path = temp.path().join("main.gof");
    let helper_display = helper_path.to_string_lossy().to_string();

    fs::write(
        &helper_path,
        "fn square(x: int) -> int:\n    return await 1\n",
    )
    .expect("helper module should be written");
    fs::write(
        &main_path,
        "import math\n\nfn main() -> int:\n    return square(9)\n",
    )
    .expect("main module should be written");

    gof_command()
        .arg("run")
        .arg(&main_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(helper_display))
        .stderr(predicate::str::contains("GOF3009"));
}

#[test]
fn gof_mod_init_writes_manifest() {
    let temp = tempdir().expect("tempdir should exist");
    gof_command()
        .args(["mod", "init", "example/app", "--dir"])
        .arg(temp.path())
        .assert()
        .success();

    let manifest = fs::read_to_string(temp.path().join("gof.mod")).expect("manifest should exist");
    assert!(manifest.contains("module = \"example/app\""));
    assert!(manifest.contains("edition = \"2026\""));
}

#[test]
fn gof_fmt_rewrites_source() {
    let temp = tempdir().expect("tempdir should exist");
    let source_path = temp.path().join("main.gof");
    fs::write(
        &source_path,
        "fn add(a:int, b:int)->int:\n    return a+b\n\nfn main()->int:\n    mut total:int=add(1,2)\n    total=total+1\n    return total\n",
    )
    .expect("source should be written");

    gof_command()
        .arg("fmt")
        .arg(&source_path)
        .assert()
        .success();

    let formatted = fs::read_to_string(&source_path).expect("formatted source should exist");
    assert_eq!(
        formatted,
        "fn add(a: int, b: int) -> int:\n    return a + b\n\nfn main() -> int:\n    mut total: int = add(1, 2)\n    total = total + 1\n    return total\n"
    );
}

#[test]
fn gof_test_runs_fixtures() {
    let fixtures = gof_conformance::workspace_root()
        .join("tests")
        .join("fixtures");
    gof_command()
        .arg("test")
        .arg(fixtures)
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn gof_build_native_emits_runnable_host_executable() {
    let temp = tempdir().expect("tempdir should exist");
    let source = gof_conformance::workspace_root()
        .join("examples")
        .join("hello_print.gof");
    let output = temp.path().join("hello-native");
    let built_binary = if cfg!(windows) {
        output.with_extension("exe")
    } else {
        output.clone()
    };

    gof_command()
        .arg("build")
        .arg(&source)
        .arg("--native")
        .arg("--output")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote native executable"));

    assert!(built_binary.exists(), "native binary should be created");

    let output = ProcessCommand::new(&built_binary)
        .output()
        .expect("native binary should execute");
    assert!(
        output.status.success(),
        "native binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("gof ready"));
    assert!(stdout.contains("42"));
    assert!(stdout.contains("7"));
}
