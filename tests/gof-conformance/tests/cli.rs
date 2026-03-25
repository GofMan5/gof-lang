use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
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
        "fn add(a, b):\n    return a+b\n\nfn main():\n    mut total=add(1,2)\n    total=total+1\n    return total\n",
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
        "fn add(a, b):\n    return a + b\n\nfn main():\n    mut total = add(1, 2)\n    total = total + 1\n    return total\n"
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
