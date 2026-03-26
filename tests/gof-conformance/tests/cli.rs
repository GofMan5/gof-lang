use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};
use std::thread;
use tempfile::tempdir;

fn gof_command() -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(gof_conformance::workspace_root())
        .args(["run", "-q", "-p", "gof-cli", "--bin", "gof", "--"]);
    command
}

fn normalize_path_for_assert(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace("\\\\?\\", "")
        .to_ascii_lowercase()
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
        .stdout(predicate::str::contains("47"));
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
