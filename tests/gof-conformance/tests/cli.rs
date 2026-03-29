use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn gof_command() -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(gof_conformance::workspace_root())
        .args(["run", "-q", "-p", "gof-cli", "--bin", "gof", "--"]);
    command
}

fn gof_binary_path() -> &'static Path {
    static GOF_BINARY: OnceLock<std::path::PathBuf> = OnceLock::new();
    GOF_BINARY.get_or_init(|| {
        let status = ProcessCommand::new("cargo")
            .current_dir(gof_conformance::workspace_root())
            .args(["build", "-q", "-p", "gof-cli", "--bin", "gof"])
            .status()
            .expect("cargo build for gof binary should run");
        assert!(status.success(), "gof binary build should succeed");

        let mut path = gof_conformance::workspace_root()
            .join("target")
            .join("debug")
            .join("gof");
        if cfg!(windows) {
            path.set_extension("exe");
        }
        path
    })
}

struct LiveOutputProcess {
    child: Child,
    stdout: Arc<Mutex<String>>,
    stderr: Arc<Mutex<String>>,
    stdout_thread: Option<thread::JoinHandle<()>>,
    stderr_thread: Option<thread::JoinHandle<()>>,
}

impl LiveOutputProcess {
    fn spawn<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut child = ProcessCommand::new(gof_binary_path())
            .current_dir(gof_conformance::workspace_root())
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("gof process should spawn");

        let stdout = Arc::new(Mutex::new(String::new()));
        let stderr = Arc::new(Mutex::new(String::new()));
        let stdout_thread = Some(spawn_output_collector(
            child.stdout.take().expect("stdout pipe should exist"),
            stdout.clone(),
        ));
        let stderr_thread = Some(spawn_output_collector(
            child.stderr.take().expect("stderr pipe should exist"),
            stderr.clone(),
        ));

        Self {
            child,
            stdout,
            stderr,
            stdout_thread,
            stderr_thread,
        }
    }

    fn wait_for_stdout(&self, needle: &str, timeout: Duration) {
        self.wait_for_buffer(&self.stdout, needle, timeout);
    }

    fn wait_for_stderr(&self, needle: &str, timeout: Duration) {
        self.wait_for_buffer(&self.stderr, needle, timeout);
    }

    fn wait_for_buffer(&self, buffer: &Arc<Mutex<String>>, needle: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let snapshot = buffer
                .lock()
                .expect("buffer mutex should not be poisoned")
                .clone();
            if snapshot.contains(needle) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "timed out waiting for {:?}\nstdout:\n{}\nstderr:\n{}",
                    needle,
                    self.stdout
                        .lock()
                        .expect("stdout mutex should not be poisoned"),
                    self.stderr
                        .lock()
                        .expect("stderr mutex should not be poisoned")
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn terminate(&mut self) {
        if self
            .child
            .try_wait()
            .expect("child status should be readable")
            .is_none()
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LiveOutputProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn spawn_output_collector<R>(mut reader: R, target: Arc<Mutex<String>>) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut chunk = [0u8; 2048];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&chunk[..read]);
                    target
                        .lock()
                        .expect("collector mutex should not be poisoned")
                        .push_str(&text);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    })
}

fn normalize_path_for_assert(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace("\\\\?\\", "")
        .to_ascii_lowercase()
}

fn parse_stdout_json(output: &[u8]) -> serde_json::Value {
    serde_json::from_slice(output).expect("stdout should contain valid JSON")
}

fn expected_http_request_len(bytes: &[u8]) -> Option<usize> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)?;
    let headers = std::str::from_utf8(&bytes[..header_end]).ok()?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("Content-Length: ")
                .or_else(|| line.strip_prefix("content-length: "))
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    Some(header_end + content_length)
}

fn write_local_package_pair(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let app_root = root.join("package_app");
    let math_root = root.join("package_math");
    fs::create_dir_all(app_root.join("src")).expect("app source root should exist");
    fs::create_dir_all(math_root.join("src")).expect("math source root should exist");
    fs::write(
        app_root.join("gof.mod"),
        "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
    )
    .expect("app manifest should exist");
    fs::write(
        app_root.join("src").join("main.gof"),
        "import package_math\n\nfn main() -> int:\n    return square(9) + square(3)\n",
    )
    .expect("app source should exist");
    fs::write(
        math_root.join("gof.mod"),
        "module = \"example/package_math\"\nedition = \"2026\"\n\n[dependencies]\n",
    )
    .expect("math manifest should exist");
    fs::write(
        math_root.join("src").join("lib.gof"),
        "fn square(value: int) -> int:\n    return value * value\n",
    )
    .expect("math library should exist");
    (app_root, math_root)
}

fn write_runtime_fail_package(root: &Path) -> std::path::PathBuf {
    let package_root = root.join("runtime_fail_package");
    fs::create_dir_all(package_root.join("src")).expect("package source root should exist");
    fs::write(
        package_root.join("gof.mod"),
        "module = \"example/runtime_fail\"\nedition = \"2026\"\n\n[dependencies]\n",
    )
    .expect("manifest should exist");
    fs::write(
        package_root.join("src").join("main.gof"),
        "fn main() -> int:\n    return 1 / 0\n",
    )
    .expect("main source should exist");
    package_root
}

fn write_library_package(root: &Path) -> std::path::PathBuf {
    let package_root = root.join("library_package");
    fs::create_dir_all(package_root.join("src")).expect("package source root should exist");
    fs::write(
        package_root.join("gof.mod"),
        "module = \"example/library\"\nedition = \"2026\"\n\n[dependencies]\n",
    )
    .expect("manifest should exist");
    fs::write(
        package_root.join("src").join("lib.gof"),
        "fn meaning() -> int:\n    return 42\n",
    )
    .expect("library source should exist");
    package_root
}

fn write_watch_script(path: &Path, body: &str) {
    fs::write(path, body).expect("watch script should be written");
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
fn gof_check_accepts_valid_example() {
    let example = gof_conformance::workspace_root()
        .join("tests")
        .join("fixtures")
        .join("pass")
        .join("hello.gof");

    gof_command().arg("check").arg(example).assert().success();
}

#[test]
fn gof_check_json_reports_compile_diagnostics() {
    let fixture = gof_conformance::workspace_root()
        .join("tests")
        .join("fixtures")
        .join("fail")
        .join("annotated_type_mismatch.gof");

    let assert = gof_command()
        .arg("check")
        .arg(&fixture)
        .arg("--json")
        .assert()
        .failure();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("\"ok\": false"),
        "stdout should report failure: {stdout}"
    );
    assert!(
        stdout.contains("\"code\": \"GOF3013\""),
        "stdout should include GOF3013: {stdout}"
    );
    assert!(
        stdout.contains("annotated_type_mismatch.gof"),
        "stdout should contain the reported source file name: {stdout}"
    );
}

#[test]
fn gof_check_json_reads_buffer_contents_from_stdin() {
    let temp = tempdir().expect("tempdir should exist");
    let source_path = temp.path().join("stdin-check.gof");
    fs::write(&source_path, "fn main() -> int:\n    return 42\n")
        .expect("source file should exist");

    let assert = gof_command()
        .arg("check")
        .arg(&source_path)
        .arg("--json")
        .arg("--stdin")
        .write_stdin("fn main() -> int:\n    return \"boom\"\n")
        .assert()
        .failure();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("\"ok\": false"),
        "stdout should report failure: {stdout}"
    );
    assert!(
        stdout.contains("\"code\": \"GOF3013\""),
        "stdout should include the type mismatch from stdin content: {stdout}"
    );
    assert!(
        stdout.contains("stdin-check.gof"),
        "stdout should keep the original source path for editor mapping: {stdout}"
    );
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
fn gof_run_executes_for_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("for_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("19"));
}

#[test]
fn gof_run_executes_break_continue_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("break_continue.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("4"));
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
fn gof_run_executes_dict_views_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("dict_views.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("28"));
}

#[test]
fn gof_run_executes_text_helpers_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("text_helpers.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("10"));
}

#[test]
fn gof_run_executes_conversion_helpers_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("conversion_helpers.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 47)"));
}

#[test]
fn gof_run_executes_sequence_helpers_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("sequence_helpers.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 15)"));
}

#[test]
fn gof_run_executes_comparison_surface_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("comparison_surface.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 42)"));
}

#[test]
fn gof_run_executes_local_package_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("package_app");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("90"));
}

#[test]
fn gof_mod_resolve_writes_lockfile_for_local_packages() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, _) = write_local_package_pair(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("gof.lock"));

    let lockfile = fs::read_to_string(app_root.join("gof.lock")).expect("lockfile should exist");
    assert!(lockfile.contains("module = \"example/package_app\""));
    assert!(lockfile.contains("module = \"example/package_math\""));
    assert!(lockfile.contains("path = \"../package_math\""));
}

#[test]
fn gof_run_requires_lockfile_for_manifest_backed_packages() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, _) = write_local_package_pair(temp.path());

    gof_command()
        .arg("run")
        .arg(&app_root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3090"))
        .stderr(predicate::str::contains("gof mod resolve"));
}

#[test]
fn gof_run_watch_reruns_single_file_scripts_after_edits() {
    let temp = tempdir().expect("tempdir should exist");
    let script = temp.path().join("watch_single.gof");
    write_watch_script(&script, "fn main() -> int:\n    return 1\n");

    let process = LiveOutputProcess::spawn([
        "run",
        "--watch",
        "--debounce-ms",
        "50",
        script.to_str().expect("script path should be valid utf-8"),
    ]);
    process.wait_for_stderr("[watch] watching for changes", Duration::from_secs(30));
    process.wait_for_stdout("1", Duration::from_secs(30));

    write_watch_script(&script, "fn main() -> int:\n    return 2\n");

    process.wait_for_stderr(
        "[watch] change detected; rerunning",
        Duration::from_secs(30),
    );
    process.wait_for_stdout("2", Duration::from_secs(30));
}

#[test]
fn gof_run_watch_preserves_trailing_args_across_reruns() {
    let temp = tempdir().expect("tempdir should exist");
    let script = temp.path().join("watch_args.gof");
    write_watch_script(
        &script,
        "fn main() -> int:\n    values = argv()\n    return len(values)\n",
    );

    let process = LiveOutputProcess::spawn([
        "run",
        "--watch",
        "--debounce-ms",
        "50",
        script.to_str().expect("script path should be valid utf-8"),
        "--",
        "alpha",
        "beta",
    ]);
    process.wait_for_stdout("2", Duration::from_secs(30));

    write_watch_script(
        &script,
        "fn main() -> int:\n    values = argv()\n    return len(values) + 1\n",
    );

    process.wait_for_stdout("3", Duration::from_secs(30));
}

#[test]
fn gof_run_watch_keeps_running_across_runtime_failures() {
    let temp = tempdir().expect("tempdir should exist");
    let script = temp.path().join("watch_fail.gof");
    write_watch_script(&script, "fn main() -> int:\n    return 1 / 0\n");

    let process = LiveOutputProcess::spawn([
        "run",
        "--watch",
        "--debounce-ms",
        "50",
        script.to_str().expect("script path should be valid utf-8"),
    ]);
    process.wait_for_stderr("GOF3068", Duration::from_secs(30));
    process.wait_for_stderr(
        "[watch] run failed; waiting for changes",
        Duration::from_secs(30),
    );

    write_watch_script(&script, "fn main() -> int:\n    return 7\n");

    process.wait_for_stdout("7", Duration::from_secs(30));
}

#[test]
fn gof_run_watch_tracks_locked_local_package_dependencies() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, math_root) = write_local_package_pair(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success();

    let process = LiveOutputProcess::spawn([
        "run",
        "--watch",
        "--debounce-ms",
        "50",
        app_root
            .to_str()
            .expect("package path should be valid utf-8"),
    ]);
    process.wait_for_stdout("90", Duration::from_secs(30));

    fs::write(
        math_root.join("src").join("lib.gof"),
        "fn square(value: int) -> int:\n    return value * value * value\n",
    )
    .expect("dependency source should update");

    process.wait_for_stdout("756", Duration::from_secs(30));
}

#[test]
fn gof_run_watch_reports_stale_lockfiles_without_stopping() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, _) = write_local_package_pair(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success();

    let process = LiveOutputProcess::spawn([
        "run",
        "--watch",
        "--debounce-ms",
        "50",
        app_root
            .to_str()
            .expect("package path should be valid utf-8"),
    ]);
    process.wait_for_stdout("90", Duration::from_secs(30));

    fs::write(
        app_root.join("gof.mod"),
        "module = \"example/package_app\"\nedition = \"2027\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
    )
    .expect("manifest should update");
    process.wait_for_stderr("GOF3091", Duration::from_secs(30));
    process.wait_for_stderr(
        "[watch] run failed; waiting for changes",
        Duration::from_secs(30),
    );

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success();

    fs::write(
        app_root.join("src").join("main.gof"),
        "import package_math\n\nfn main() -> int:\n    return square(9) + square(3) + 1\n",
    )
    .expect("main source should update after lock refresh");

    process.wait_for_stdout("91", Duration::from_secs(30));
}

#[test]
fn gof_run_watch_rejects_library_targets() {
    let temp = tempdir().expect("tempdir should exist");
    let package_root = write_library_package(temp.path());

    gof_command()
        .arg("run")
        .arg("--watch")
        .arg(package_root.join("src").join("lib.gof"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3102"))
        .stderr(predicate::str::contains("executable target"));
}

#[test]
fn gof_run_rejects_stale_package_lockfiles() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, _) = write_local_package_pair(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success();

    fs::write(
        app_root.join("gof.mod"),
        "module = \"example/package_app\"\nedition = \"2027\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
    )
    .expect("manifest should be updated");

    gof_command()
        .arg("run")
        .arg(&app_root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3091"))
        .stderr(predicate::str::contains("stale"));
}

#[test]
fn gof_run_executes_range_helpers_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("range_helpers.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("40"));
}

#[test]
fn gof_run_executes_numeric_surface_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("numeric_surface.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("2"));
}

#[test]
fn gof_run_executes_payload_match_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("payload_match.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
}

#[test]
fn gof_run_executes_result_flow_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("result_flow.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
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
fn gof_run_executes_channel_capacity_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("channel_capacity.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 12)"));
}

#[test]
fn gof_run_executes_select_round_robin_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("select_round_robin.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 16)"));
}

#[test]
fn gof_run_executes_select_default_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("select_default.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 7)"));
}

#[test]
fn gof_run_executes_select_send_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("select_send.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 10)"));
}

#[test]
fn gof_run_executes_timeout_cancellation_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("timeout_cancellation.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("42"));
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
fn gof_run_executes_line_io_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("line_io.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("19"));
}

#[test]
fn gof_run_executes_stdin_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("stdin_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .write_stdin("alpha\nbeta\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Result.Ok(value: chars=11 first=alpha lines=2)",
        ));
}

#[test]
fn gof_run_executes_time_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("time_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 1)"));
}

#[test]
fn gof_run_executes_base64_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("base64_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 12)"));
}

#[test]
fn gof_run_executes_yaml_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("yaml_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 17)"));
}

#[test]
fn gof_run_executes_csv_inventory_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("csv_inventory.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 8)"));
}

#[test]
fn gof_run_executes_config_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("config_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 17)"));
}

#[test]
fn gof_run_executes_template_report_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("template_report.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Result.Ok(value: service=alpha port=7 workers=5)",
        ));
}

#[test]
fn gof_run_executes_process_capture_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("process_capture.gof");
    let helper = std::env::current_exe().expect("current test executable should exist");

    gof_command()
        .arg("run")
        .arg(example)
        .env("GOF_PROCESS_EXAMPLE", &helper)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 7)"));
}

#[test]
fn gof_run_executes_runtime_ops_example_with_args_env_and_fs_helpers() {
    let temp = tempdir().expect("tempdir should exist");
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("runtime_ops.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .arg("--")
        .args(["alpha", "beta"])
        .env("GOF_RUNTIME_DIR", temp.path())
        .env("GOF_RUNTIME_MODE", "bot")
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 6)"));

    assert!(temp.path().join("sample.txt").exists());
}

#[test]
fn gof_run_executes_channel_lifecycle_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("channel_lifecycle.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("7"));
}

#[test]
fn gof_run_executes_task_result_propagation_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("task_result_propagation.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "RuntimeError.TaskFailed(message: GOF3068: `/` by zero is not allowed)",
        ));
}

#[test]
fn gof_run_executes_await_result_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("await_result.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("36 | RuntimeError.TaskFailed("));
}

#[test]
fn gof_run_executes_await_result_cancellation_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("await_result_cancellation.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("RuntimeError.Cancelled"));
}

#[test]
fn gof_run_executes_telegram_long_polling_example_against_fake_api() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("telegram_long_polling.gof");
    let observed_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener addr should exist");
    let observed_requests_thread = observed_requests.clone();

    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 4096];
            let mut request_bytes = Vec::new();
            loop {
                let size = stream
                    .read(&mut buffer)
                    .expect("request should be readable");
                if size == 0 {
                    break;
                }
                request_bytes.extend_from_slice(&buffer[..size]);
                if let Some(total_len) = expected_http_request_len(&request_bytes) {
                    if request_bytes.len() >= total_len {
                        request_bytes.truncate(total_len);
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request_bytes).to_string();
            let request_line = request.lines().next().unwrap_or_default().to_string();
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            observed_requests_thread
                .lock()
                .expect("requests mutex should not be poisoned")
                .push(request.clone());

            let body = if path.contains("/getUpdates") {
                "{\"ok\":true,\"result\":[{\"update_id\":123,\"message\":{\"chat\":{\"id\":777},\"text\":\"/ping\"}}]}".to_string()
            } else if path.contains("/sendMessage") {
                "{\"ok\":true,\"result\":{\"message_id\":1}}".to_string()
            } else {
                "{\"ok\":false,\"result\":[]}".to_string()
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        }
    });

    gof_command()
        .arg("run")
        .arg(example)
        .env("TELEGRAM_BOT_TOKEN", "test-token")
        .env("GOF_TELEGRAM_API_BASE", format!("http://{address}"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 123)"));

    server.join().expect("server thread should exit");

    let requests = observed_requests
        .lock()
        .expect("requests mutex should not be poisoned")
        .clone();
    assert!(
        requests
            .iter()
            .any(|request| request.contains("GET /bottest-token/getUpdates")),
        "expected getUpdates request, got {requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.contains("POST /bottest-token/sendMessage HTTP/1.1")),
        "expected sendMessage POST request, got {requests:?}"
    );
    assert!(
        requests.iter().any(|request| {
            request.contains("{\"chat_id\":777,\"text\":\"pong\"}")
                && request.contains("Content-Type: application/json")
        }),
        "expected JSON sendMessage payload, got {requests:?}"
    );
}

#[test]
fn gof_run_executes_http_request_report_example_against_fake_api() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("http_request_report.gof");
    let observed_requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener addr should exist");
    let observed_requests_thread = observed_requests.clone();

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("request should arrive");
        let mut buffer = [0_u8; 4096];
        let mut request_bytes = Vec::new();
        loop {
            let size = stream
                .read(&mut buffer)
                .expect("request should be readable");
            if size == 0 {
                break;
            }
            request_bytes.extend_from_slice(&buffer[..size]);
            if let Some(total_len) = expected_http_request_len(&request_bytes) {
                if request_bytes.len() >= total_len {
                    request_bytes.truncate(total_len);
                    break;
                }
            }
        }
        let request = String::from_utf8_lossy(&request_bytes).to_string();
        observed_requests_thread
            .lock()
            .expect("requests mutex should not be poisoned")
            .push(request);

        let body = "accepted";
        let response = format!(
            "HTTP/1.1 202 Accepted\r\nContent-Type: text/plain; charset=utf-8\r\nX-Request-Id: req-42\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be written");
    });

    gof_command()
        .arg("run")
        .arg(example)
        .env("GOF_HTTP_REQUEST_BASE", format!("http://{address}"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 216)"));

    server.join().expect("server thread should exit");

    let requests = observed_requests
        .lock()
        .expect("requests mutex should not be poisoned")
        .clone();
    assert!(
        requests
            .iter()
            .any(|request| request.contains("POST /inspect HTTP/1.1")),
        "expected POST request, got {requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.contains("Authorization: Bearer demo-token")),
        "expected Authorization header, got {requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.contains("X-Trace-Id: trace-7")),
        "expected custom trace header, got {requests:?}"
    );
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

    let assert = gof_command().arg("run").arg(&main_path).assert().failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    let normalized_stderr = stderr.replace("\\\\?\\", "").to_ascii_lowercase();
    let normalized_helper_path = normalize_path_for_assert(&helper_path);

    assert!(
        normalized_stderr.contains(&normalized_helper_path),
        "stderr should contain imported file path.\nexpected path: {normalized_helper_path}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("GOF3009"),
        "stderr should contain GOF3009.\nstderr: {stderr}"
    );
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
    let main_source =
        fs::read_to_string(temp.path().join("src").join("main.gof")).expect("main should exist");
    assert!(manifest.contains("module = \"example/app\""));
    assert!(manifest.contains("edition = \"2026\""));
    assert_eq!(main_source, "fn main() -> int:\n    return 0\n");
}

#[test]
fn gof_mod_init_and_resolve_support_a_new_local_package_pair() {
    let temp = tempdir().expect("tempdir should exist");
    let app_root = temp.path().join("app");
    let math_root = temp.path().join("math");

    gof_command()
        .args(["mod", "init", "example/package_app", "--dir"])
        .arg(&app_root)
        .assert()
        .success();
    gof_command()
        .args(["mod", "init", "example/package_math", "--dir"])
        .arg(&math_root)
        .assert()
        .success();

    fs::remove_file(math_root.join("src").join("main.gof")).expect("math main should be removed");
    fs::write(
        math_root.join("src").join("lib.gof"),
        "fn square(value: int) -> int:\n    return value * value\n",
    )
    .expect("math library should exist");
    fs::write(
        app_root.join("gof.mod"),
        "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../math\" }\n",
    )
    .expect("app manifest should be rewritten");
    fs::write(
        app_root.join("src").join("main.gof"),
        "import package_math\n\nfn main() -> int:\n    return square(9)\n",
    )
    .expect("app main should be rewritten");

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("gof.lock"));

    gof_command()
        .arg("run")
        .arg(&app_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("81"));
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
fn gof_test_executes_manifest_backed_executable_targets_honestly() {
    let temp = tempdir().expect("tempdir should exist");
    let package_root = write_runtime_fail_package(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&package_root)
        .assert()
        .success();

    gof_command()
        .arg("test")
        .arg(&package_root)
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3068"))
        .stdout(predicate::str::contains("0 passed; 1 failed"));
}

#[test]
fn gof_test_json_reports_package_target_diagnostics() {
    let temp = tempdir().expect("tempdir should exist");
    let package_root = write_runtime_fail_package(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&package_root)
        .assert()
        .success();

    let assert = gof_command()
        .arg("test")
        .arg("--json")
        .arg(&package_root)
        .assert()
        .failure();

    let report = parse_stdout_json(&assert.get_output().stdout);
    assert_eq!(report["ok"], false);
    assert_eq!(report["summary"]["failed"], 1);

    let package_event = report["events"]
        .as_array()
        .and_then(|events| events.iter().find(|event| event["kind"] == "package"))
        .expect("package event should exist");
    assert_eq!(package_event["status"], "failed");
    assert!(package_event["sourcePath"]
        .as_str()
        .map(|path| path.contains("src/main.gof"))
        == Some(true));
    assert!(package_event["diagnostics"]
        .as_array()
        .map(|diagnostics| diagnostics.iter().any(|diagnostic| diagnostic["code"] == "GOF3068"))
        == Some(true));
}

#[test]
fn gof_test_compile_checks_library_package_targets_without_execution() {
    let temp = tempdir().expect("tempdir should exist");
    let package_root = write_library_package(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&package_root)
        .assert()
        .success();

    gof_command()
        .arg("test")
        .arg(&package_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("test result: 1 passed; 0 failed"))
        .stdout(predicate::str::contains("src/lib.gof"));
}

#[test]
fn gof_test_recognizes_canonicalized_package_roots() {
    let temp = tempdir().expect("tempdir should exist");
    let package_root = write_library_package(temp.path());

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&package_root)
        .assert()
        .success();

    let canonical_root = package_root
        .canonicalize()
        .expect("canonical package root should exist");
    gof_command()
        .arg("test")
        .arg(&canonical_root)
        .assert()
        .success()
        .stdout(predicate::str::contains("src/lib.gof"))
        .stdout(predicate::str::contains("test result: 1 passed; 0 failed"))
        .stdout(predicate::str::contains("fixture").not());
}

#[test]
fn gof_check_rejects_local_modules_that_conflict_with_reserved_stdlib_imports() {
    let temp = tempdir().expect("tempdir should exist");
    let main_path = temp.path().join("main.gof");
    let local_http = temp.path().join("http.gof");

    fs::write(&local_http, "fn helper() -> int:\n    return 1\n")
        .expect("local http module should be written");
    fs::write(
        &main_path,
        "import http\n\nfn main() -> int:\n    return 7\n",
    )
    .expect("main module should be written");

    gof_command()
        .arg("check")
        .arg(&main_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3108"))
        .stderr(predicate::str::contains("reserved stdlib import `http`"));
}

#[test]
fn gof_run_executes_shipped_stdlib_import_smoke_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("stdlib_imports.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("0"));
}

#[test]
fn gof_run_executes_bytes_stream_roundtrip_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("bytes_stream_roundtrip.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 20)"));
}

#[test]
fn gof_run_executes_tcp_roundtrip_example() {
    let example = gof_conformance::workspace_root()
        .join("examples")
        .join("tcp_roundtrip.gof");

    gof_command()
        .arg("run")
        .arg(example)
        .assert()
        .success()
        .stdout(predicate::str::contains("Result.Ok(value: 10)"));
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

#[test]
fn gof_build_native_preserves_same_directory_imports_across_working_directories() {
    let temp = tempdir().expect("tempdir should exist");
    let helper_path = temp.path().join("math.gof");
    let main_path = temp.path().join("main.gof");
    let output = temp.path().join("import-native");
    let built_binary = if cfg!(windows) {
        output.with_extension("exe")
    } else {
        output.clone()
    };

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
        .arg("build")
        .arg(&main_path)
        .arg("--native")
        .arg("--output")
        .arg(&output)
        .assert()
        .success();

    let run_dir = temp.path().join("run-from-here");
    fs::create_dir_all(&run_dir).expect("run directory should exist");
    let execution = ProcessCommand::new(&built_binary)
        .current_dir(&run_dir)
        .output()
        .expect("native binary should execute");
    assert!(
        execution.status.success(),
        "native binary failed: {}",
        String::from_utf8_lossy(&execution.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&execution.stdout).trim(), "81");
}

#[test]
fn gof_build_native_preserves_manifest_backed_package_imports_across_working_directories() {
    let temp = tempdir().expect("tempdir should exist");
    let (app_root, _) = write_local_package_pair(temp.path());
    let output = temp.path().join("package-native");
    let built_binary = if cfg!(windows) {
        output.with_extension("exe")
    } else {
        output.clone()
    };

    gof_command()
        .args(["mod", "resolve", "--dir"])
        .arg(&app_root)
        .assert()
        .success();

    gof_command()
        .arg("build")
        .arg(&app_root)
        .arg("--native")
        .arg("--output")
        .arg(&output)
        .assert()
        .success();

    let run_dir = temp.path().join("other-cwd");
    fs::create_dir_all(&run_dir).expect("run directory should exist");
    let execution = ProcessCommand::new(&built_binary)
        .current_dir(&run_dir)
        .output()
        .expect("native binary should execute");
    assert!(
        execution.status.success(),
        "native binary failed: {}",
        String::from_utf8_lossy(&execution.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&execution.stdout).trim(), "90");
}

#[test]
fn gof_build_native_preserves_shipped_stdlib_imports_across_working_directories() {
    let temp = tempdir().expect("tempdir should exist");
    let main_path = temp.path().join("main.gof");
    let output = temp.path().join("stdlib-native");
    let built_binary = if cfg!(windows) {
        output.with_extension("exe")
    } else {
        output.clone()
    };

    fs::write(
        &main_path,
        "import bytes\nimport http\nimport io\nimport net\nimport time\n\nfn main() -> int:\n    return 7\n",
    )
    .expect("main module should be written");

    gof_command()
        .arg("build")
        .arg(&main_path)
        .arg("--native")
        .arg("--output")
        .arg(&output)
        .assert()
        .success();

    let run_dir = temp.path().join("stdlib-other-cwd");
    fs::create_dir_all(&run_dir).expect("run directory should exist");
    let execution = ProcessCommand::new(&built_binary)
        .current_dir(&run_dir)
        .output()
        .expect("native binary should execute");
    assert!(
        execution.status.success(),
        "native binary failed: {}",
        String::from_utf8_lossy(&execution.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&execution.stdout).trim(), "7");
}

#[test]
fn gof_test_discovers_language_level_tests_and_updates_snapshots() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("math_test.gof"),
        "import testing\n\ntest fn truthy_case(t: TestContext):\n    t.true(true, \"expected truth\")\n\ntest fn snapshot_case(t: TestContext):\n    t.match_snapshot(\"rendered\", \"hello from snapshot\")\n",
    )
    .expect("language-level test file should exist");

    gof_command()
        .arg("test")
        .arg("--update-snapshots")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ok").and(predicate::str::contains("truthy_case")))
        .stdout(predicate::str::contains("ok").and(predicate::str::contains("snapshot_case")));

    let snapshot = temp
        .path()
        .join("tests")
        .join("snapshots")
        .join("tests")
        .join("math_test")
        .join("snapshot-case--rendered.snap");
    assert!(
        snapshot.is_file(),
        "snapshot should be created at {}",
        snapshot.display()
    );

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test result: 2 passed; 0 failed"));
}

#[test]
fn gof_test_lists_language_level_tests() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("list_test.gof"),
        "import testing\n\ntest fn alpha_case(t: TestContext):\n    t.true(true, \"alpha\")\n\ntest fn beta_case(t: TestContext):\n    t.true(true, \"beta\")\n",
    )
    .expect("language-level test file should exist");

    gof_command()
        .arg("test")
        .arg("--list")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha_case"))
        .stdout(predicate::str::contains("beta_case"))
        .stdout(predicate::str::contains("listed 2"));
}

#[test]
fn gof_test_reports_skip_and_todo_statuses() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("status_test.gof"),
        "import testing\n\ntest fn skipped_case(t: TestContext):\n    t.skip(\"waiting for network\")\n\ntest fn todo_case(t: TestContext):\n    t.todo(\"pending assertions\")\n",
    )
    .expect("status language-level test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("skip "))
        .stdout(predicate::str::contains("todo "))
        .stdout(predicate::str::contains(
            "0 passed; 0 failed; 1 skipped; 1 todo",
        ));
}

#[test]
fn gof_test_json_reports_language_statuses_and_compile_failures() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("status_test.gof"),
        "import testing\n\ntest fn passing_case(t: TestContext):\n    print(\"alpha\")\n    t.true(true, \"expected passing case\")\n\ntest fn skipped_case(t: TestContext):\n    t.skip(\"waiting for network\")\n\ntest fn todo_case(t: TestContext):\n    t.todo(\"pending assertions\")\n",
    )
    .expect("status test file should exist");
    fs::write(
        tests_root.join("broken_test.gof"),
        "test fn broken_case(repo):\n    return 0\n",
    )
    .expect("broken test file should exist");

    let assert = gof_command()
        .arg("test")
        .arg("--json")
        .arg(temp.path())
        .assert()
        .failure();

    let report = parse_stdout_json(&assert.get_output().stdout);
    assert_eq!(report["schema"], "gof.test.report/v1");
    assert_eq!(report["ok"], false);
    assert_eq!(report["summary"]["passed"], 1);
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["summary"]["skipped"], 1);
    assert_eq!(report["summary"]["todo"], 1);

    let events = report["events"]
        .as_array()
        .expect("events should be an array");
    assert!(events.iter().any(|event| {
        event["kind"] == "language-test"
            && event["status"] == "passed"
            && event["id"].as_str().map(|id| id.contains("passing_case")) == Some(true)
            && event["stdout"] == "alpha\n"
    }));
    assert!(events.iter().any(|event| {
        event["kind"] == "language-test"
            && event["status"] == "skipped"
            && event["message"] == "waiting for network"
    }));
    assert!(events.iter().any(|event| {
        event["kind"] == "language-test"
            && event["status"] == "todo"
            && event["message"] == "pending assertions"
    }));
    assert!(events.iter().any(|event| {
        event["kind"] == "language-compile"
            && event["status"] == "failed"
            && event["diagnostics"]
                .as_array()
                .map(|diagnostics| diagnostics.iter().any(|diagnostic| diagnostic["code"] == "GOF3113"))
                == Some(true)
    }));
}

#[test]
fn gof_test_resolves_typed_fixtures_and_reuses_module_scope() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    let counter_path = temp
        .path()
        .join("counter.txt")
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(
        tests_root.join("fixture_test.gof"),
        format!(
            "import testing\n\nfixture(module) fn shared_counter() -> Result[int, RuntimeError]:\n    path = \"{counter_path}\"\n    if exists(path):\n        current_text = read_file(path)?\n        current = parse_int(current_text)?\n        next = current + 1\n        write_file(path, to_string(next))\n        return Result.Ok(next)\n    write_file(path, \"1\")\n    return Result.Ok(1)\n\nfixture(test) fn temp_root(t: TestContext) -> TempDir:\n    return t.temp_dir()\n\ntest fn first(shared_counter: int, temp_root: TempDir, t: TestContext):\n    t.equal(shared_counter, 1, \"expected cached module fixture value\")\n    t.true(exists(temp_root.path()), \"expected injected test fixture temp dir\")\n\ntest fn second(shared_counter: int, t: TestContext):\n    t.equal(shared_counter, 1, \"expected cached module fixture value\")\n"
        ),
    )
    .expect("fixture-backed language-level test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ok ").and(predicate::str::contains(
            "fixture_test.gof::first",
        )))
        .stdout(predicate::str::contains("ok ").and(predicate::str::contains(
            "fixture_test.gof::second",
        )))
        .stdout(predicate::str::contains("test result: 2 passed; 0 failed"));

    assert_eq!(
        fs::read_to_string(temp.path().join("counter.txt"))
            .expect("counter file should exist after fixture execution"),
        "1"
    );
}

#[test]
fn gof_test_runs_fixture_cleanup_hooks_for_test_and_module_scope() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    let test_marker_path = temp
        .path()
        .join("test-fixture-cleanup.txt")
        .to_string_lossy()
        .replace('\\', "/");
    let module_marker_path = temp
        .path()
        .join("module-fixture-cleanup.txt")
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(
        tests_root.join("cleanup_test.gof"),
        format!(
            "import testing\n\nstruct TestProbe:\n    dir_path: string\n    marker_path: string\n\nfn TestProbe.cleanup(self: TestProbe) -> Result[unit, RuntimeError]:\n    if exists(self.dir_path):\n        if exists(self.marker_path):\n            current = read_file(self.marker_path)?\n            return write_file(self.marker_path, current + \"alive\\n\")\n        return write_file(self.marker_path, \"alive\\n\")\n    if exists(self.marker_path):\n        current = read_file(self.marker_path)?\n        return write_file(self.marker_path, current + \"missing\\n\")\n    return write_file(self.marker_path, \"missing\\n\")\n\nstruct ModuleProbe:\n    marker_path: string\n\nfn ModuleProbe.cleanup(self: ModuleProbe) -> Result[unit, RuntimeError]:\n    if exists(self.marker_path):\n        current = read_file(self.marker_path)?\n        return write_file(self.marker_path, current + \"module\\n\")\n    return write_file(self.marker_path, \"module\\n\")\n\nfixture(module) fn shared_probe() -> ModuleProbe:\n    return ModuleProbe(\"{module_marker_path}\")\n\nfixture(test) fn temp_probe(t: TestContext) -> TestProbe:\n    dir = t.temp_dir()\n    return TestProbe(dir.path(), \"{test_marker_path}\")\n\ntest fn first(shared_probe: ModuleProbe, temp_probe: TestProbe, t: TestContext):\n    t.true(exists(temp_probe.dir_path), \"expected injected test fixture temp dir\")\n\ntest fn second(shared_probe: ModuleProbe, temp_probe: TestProbe, t: TestContext):\n    t.true(exists(temp_probe.dir_path), \"expected injected test fixture temp dir\")\n"
        ),
    )
    .expect("fixture cleanup test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("cleanup_test.gof::first"))
        .stdout(predicate::str::contains("cleanup_test.gof::second"))
        .stdout(predicate::str::contains("test result: 2 passed; 0 failed"));

    assert_eq!(
        fs::read_to_string(temp.path().join("test-fixture-cleanup.txt"))
            .expect("test fixture cleanup marker should exist"),
        "alive\nalive\n"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("module-fixture-cleanup.txt"))
            .expect("module fixture cleanup marker should exist"),
        "module\n"
    );
}

#[test]
fn gof_test_fails_when_fixture_cleanup_hook_fails() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("broken_cleanup_test.gof"),
        "import testing\n\nstruct BrokenProbe:\n    marker: string\n\nfn BrokenProbe.cleanup(self: BrokenProbe) -> Result[unit, RuntimeError]:\n    return Result.Err(RuntimeError.Io(\"cleanup failed\"))\n\nfixture(test) fn broken() -> BrokenProbe:\n    return BrokenProbe(\"marker\")\n\ntest fn uses_fixture(broken: BrokenProbe, t: TestContext):\n    t.true(true, \"expected primary test body to pass\")\n",
    )
    .expect("broken cleanup test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "fixture `broken` cleanup returned Result.Err",
        ))
        .stdout(predicate::str::contains("test result: 0 passed; 1 failed"));
}

#[test]
fn gof_test_surfaces_invalid_typed_fixture_dependencies() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("broken_fixture_test.gof"),
        "fixture(module) fn shared_total() -> string:\n    return \"41\"\n\ntest fn broken_case(shared_total: int):\n    return 0\n",
    )
    .expect("broken fixture-backed test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3123"))
        .stderr(predicate::str::contains("shared_total"));
}

#[test]
fn gof_test_runs_opt_in_doctests() {
    let temp = tempdir().expect("tempdir should exist");
    fs::write(
        temp.path().join("README.md"),
        "# Sample\n\n```gof doctest\nfn main() -> int:\n    return 7\n```\n\n```gof doctest no_run\nstruct Point:\n    x: int\n```\n\n```gof doctest compile_fail\nfn main() -> int:\n    return \"oops\"\n```\n\n```gof doctest runtime_fail\nfn main() -> int:\n    return 1 / 0\n```\n",
    )
    .expect("markdown doctest file should exist");

    gof_command()
        .arg("test")
        .arg("--docs")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("README.md:4::doctest#1"))
        .stdout(predicate::str::contains("README.md:9::doctest-no-run#2"))
        .stdout(predicate::str::contains(
            "README.md:14::doctest-compile-fail#3",
        ))
        .stdout(predicate::str::contains(
            "README.md:19::doctest-runtime-fail#4",
        ))
        .stdout(predicate::str::contains(
            "4 passed; 0 failed; 0 skipped; 0 todo",
        ));
}

#[test]
fn gof_test_json_reports_doctests_and_product_fixtures() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    let ui_root = tests_root.join("ui");
    fs::create_dir_all(&ui_root).expect("ui root should exist");
    fs::write(
        temp.path().join("README.md"),
        "# Sample\n\n```gof doctest\nfn main() -> int:\n    return 7\n```\n",
    )
    .expect("README should exist");
    fs::write(
        ui_root.join("type_mismatch.gof"),
        "fn main() -> int:\n    return \"oops\"\n",
    )
    .expect("ui fixture should exist");

    let assert = gof_command()
        .arg("test")
        .arg("--json")
        .arg("--docs")
        .arg("--update-snapshots")
        .arg(temp.path())
        .assert()
        .success();

    let report = parse_stdout_json(&assert.get_output().stdout);
    assert_eq!(report["ok"], true);
    assert_eq!(report["summary"]["passed"], 2);

    let events = report["events"]
        .as_array()
        .expect("events should be an array");
    let doctest_event = events
        .iter()
        .find(|event| event["kind"] == "doctest")
        .expect("doctest event should exist");
    assert_eq!(doctest_event["status"], "passed");
    assert_eq!(doctest_event["stdout"], "7\n");

    let fixture_event = events
        .iter()
        .find(|event| event["kind"] == "fixture")
        .expect("fixture event should exist");
    assert_eq!(fixture_event["status"], "passed");
    assert!(fixture_event["stderr"]
        .as_str()
        .map(|stderr| stderr.contains("tests/ui/type_mismatch.gof"))
        == Some(true));
    assert!(fixture_event["diagnostics"]
        .as_array()
        .map(|diagnostics| diagnostics.iter().any(|diagnostic| diagnostic["code"] == "GOF3013"))
        == Some(true));
}

#[test]
fn gof_test_lists_opt_in_doctests() {
    let temp = tempdir().expect("tempdir should exist");
    fs::write(
        temp.path().join("guide.md"),
        "# Guide\n\n```gof doctest\nfn main() -> int:\n    return 1\n```\n\n```gof doctest no_run\nimport testing\n\ntest fn sample(t: TestContext):\n    t.true(true, \"ok\")\n```\n",
    )
    .expect("markdown doctest file should exist");

    gof_command()
        .arg("test")
        .arg("--docs")
        .arg("--list")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("guide.md:4::doctest#1"))
        .stdout(predicate::str::contains("guide.md:9::doctest-no-run#2"))
        .stdout(predicate::str::contains("listed 2"));
}

#[test]
fn gof_test_surfaces_invalid_test_signatures() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    fs::create_dir_all(&tests_root).expect("tests root should exist");
    fs::write(
        tests_root.join("broken_test.gof"),
        "test fn broken_case(repo):\n    return 0\n",
    )
    .expect("broken language-level test file should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("GOF3113"))
        .stderr(predicate::str::contains("broken_case"));
}

#[test]
fn gof_test_runs_product_ui_and_runtime_fixtures_and_reuses_recorded_artifacts() {
    let temp = tempdir().expect("tempdir should exist");
    let tests_root = temp.path().join("tests");
    let ui_root = tests_root.join("ui");
    let runtime_root = tests_root.join("runtime");
    let runtime_fail_root = tests_root.join("runtime-fail");
    fs::create_dir_all(&ui_root).expect("ui root should exist");
    fs::create_dir_all(&runtime_root).expect("runtime root should exist");
    fs::create_dir_all(&runtime_fail_root).expect("runtime-fail root should exist");

    fs::write(
        ui_root.join("type_mismatch.gof"),
        "fn main() -> int:\n    return \"oops\"\n",
    )
    .expect("ui fixture should exist");
    fs::write(
        runtime_root.join("hello.gof"),
        "fn main() -> int:\n    print(\"hi\")\n    return 7\n",
    )
    .expect("runtime fixture should exist");
    fs::write(
        runtime_fail_root.join("division_by_zero.gof"),
        "fn main() -> int:\n    return 1 / 0\n",
    )
    .expect("runtime-fail fixture should exist");

    gof_command()
        .arg("test")
        .arg("--update-snapshots")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("tests/ui/type_mismatch.gof"))
        .stdout(predicate::str::contains("tests/runtime/hello.gof"))
        .stdout(predicate::str::contains("tests/runtime-fail/division_by_zero.gof"))
        .stdout(predicate::str::contains("test result: 3 passed; 0 failed"));

    assert_eq!(
        fs::read_to_string(ui_root.join("type_mismatch.diag"))
            .expect("ui diag artifact should exist"),
        "GOF3013\n"
    );
    assert_eq!(
        fs::read_to_string(ui_root.join("type_mismatch.exit"))
            .expect("ui exit artifact should exist"),
        "1\n"
    );
    let ui_stderr =
        fs::read_to_string(ui_root.join("type_mismatch.stderr")).expect("ui stderr should exist");
    assert!(
        ui_stderr.contains("tests/ui/type_mismatch.gof"),
        "ui stderr should use relative fixture paths: {ui_stderr}"
    );
    assert!(
        ui_stderr.contains("GOF3013"),
        "ui stderr should contain the diagnostic code: {ui_stderr}"
    );

    assert_eq!(
        fs::read_to_string(runtime_root.join("hello.stdout"))
            .expect("runtime stdout artifact should exist"),
        "hi\n7\n"
    );
    assert_eq!(
        fs::read_to_string(runtime_root.join("hello.exit"))
            .expect("runtime exit artifact should exist"),
        "0\n"
    );

    assert_eq!(
        fs::read_to_string(runtime_fail_root.join("division_by_zero.diag"))
            .expect("runtime-fail diag artifact should exist"),
        "GOF3068\n"
    );
    assert_eq!(
        fs::read_to_string(runtime_fail_root.join("division_by_zero.exit"))
            .expect("runtime-fail exit artifact should exist"),
        "1\n"
    );
    let runtime_fail_stderr = fs::read_to_string(runtime_fail_root.join("division_by_zero.stderr"))
        .expect("runtime-fail stderr should exist");
    assert!(
        runtime_fail_stderr.contains("tests/runtime-fail/division_by_zero.gof"),
        "runtime-fail stderr should use relative fixture paths: {runtime_fail_stderr}"
    );
    assert!(
        runtime_fail_stderr.contains("GOF3068"),
        "runtime-fail stderr should contain the diagnostic code: {runtime_fail_stderr}"
    );

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test result: 3 passed; 0 failed"));
}

#[test]
fn gof_test_rewrites_product_fixture_artifacts_with_update_snapshots() {
    let temp = tempdir().expect("tempdir should exist");
    let runtime_root = temp.path().join("tests").join("runtime");
    fs::create_dir_all(&runtime_root).expect("runtime root should exist");
    fs::write(
        runtime_root.join("hello.gof"),
        "fn main() -> int:\n    print(\"hi\")\n    return 7\n",
    )
    .expect("runtime fixture should exist");
    fs::write(runtime_root.join("hello.stdout"), "stale\n").expect("stale stdout should exist");
    fs::write(runtime_root.join("hello.exit"), "9\n").expect("stale exit should exist");
    fs::write(runtime_root.join("hello.stderr"), "stale stderr\n")
        .expect("stale stderr should exist");

    gof_command()
        .arg("test")
        .arg(temp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("fixture artifact mismatch"))
        .stderr(predicate::str::contains("hello.stdout"));

    gof_command()
        .arg("test")
        .arg("--update-snapshots")
        .arg(temp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test result: 1 passed; 0 failed"));

    assert_eq!(
        fs::read_to_string(runtime_root.join("hello.stdout"))
            .expect("runtime stdout artifact should be updated"),
        "hi\n7\n"
    );
    assert_eq!(
        fs::read_to_string(runtime_root.join("hello.exit"))
            .expect("runtime exit artifact should be updated"),
        "0\n"
    );
    assert!(
        !runtime_root.join("hello.stderr").exists(),
        "stale stderr artifact should be removed on update"
    );
    assert!(
        !runtime_root.join("hello.diag").exists(),
        "stale diag artifact should be absent for successful runtime fixtures"
    );
}
