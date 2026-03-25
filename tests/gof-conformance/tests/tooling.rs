#[cfg(windows)]
use assert_cmd::Command;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::process::Command as ProcessCommand;
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::Duration;
#[cfg(windows)]
use tempfile::tempdir;

#[cfg(windows)]
fn read_user_path() -> String {
    let output = ProcessCommand::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path', 'User')",
        ])
        .output()
        .expect("powershell should be available");

    assert!(
        output.status.success(),
        "powershell path query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[cfg(windows)]
#[test]
fn windows_install_core_installs_payload_into_target_root() {
    let root = gof_conformance::workspace_root();
    let temp = tempdir().expect("tempdir should exist");
    let payload_dir = temp.path().join("payload");
    let install_dir = temp.path().join("install-root");
    fs::create_dir_all(&payload_dir).expect("payload dir should exist");

    fs::write(payload_dir.join("gof.exe"), b"fake-exe").expect("binary payload should be written");
    fs::write(payload_dir.join("README.md"), "readme").expect("readme payload should be written");
    fs::write(payload_dir.join("LICENSE"), "license").expect("license payload should be written");
    fs::copy(
        root.join("scripts")
            .join("windows-installer")
            .join("uninstall-gof.ps1"),
        payload_dir.join("uninstall-gof.ps1"),
    )
    .expect("uninstall payload should be copied");

    let install_script = root
        .join("scripts")
        .join("windows-installer")
        .join("install-gof.ps1");

    Command::new("powershell")
        .current_dir(&root)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&install_script)
        .args(["-Version", "0.0.0-test"])
        .args(["-PayloadRoot"])
        .arg(&payload_dir)
        .args(["-InstallRoot"])
        .arg(&install_dir)
        .arg("-SkipPathUpdate")
        .arg("-SkipUninstallRegistration")
        .arg("-Quiet")
        .assert()
        .success();

    assert!(install_dir.join("bin").join("gof.exe").exists());
    assert!(install_dir.join("README.md").exists());
    assert!(install_dir.join("LICENSE").exists());
    assert!(install_dir.join("uninstall-gof.ps1").exists());
    assert_eq!(
        fs::read_to_string(install_dir.join("VERSION.txt")).expect("version file should exist"),
        "0.0.0-test\r\n"
    );
}

#[cfg(windows)]
#[test]
fn build_windows_installer_creates_setup_executable() {
    let root = gof_conformance::workspace_root();
    let temp = tempdir().expect("tempdir should exist");
    let payload_dir = temp.path().join("payload");
    let output_path = temp.path().join("gof-setup.exe");
    fs::create_dir_all(&payload_dir).expect("payload dir should exist");

    fs::write(payload_dir.join("gof.exe"), b"fake-exe").expect("binary payload should be written");
    fs::write(payload_dir.join("README.md"), "readme").expect("readme payload should be written");
    fs::write(payload_dir.join("LICENSE"), "license").expect("license payload should be written");

    let build_script = root.join("scripts").join("build-windows-installer.ps1");

    Command::new("powershell")
        .current_dir(&root)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&build_script)
        .args(["-PayloadDir"])
        .arg(&payload_dir)
        .args(["-OutputPath"])
        .arg(&output_path)
        .args(["-Version", "0.0.0-test"])
        .assert()
        .success();

    assert!(output_path.exists());
    let metadata = fs::metadata(output_path).expect("setup exe should exist");
    assert!(metadata.len() > 0);
}

#[cfg(windows)]
#[test]
fn windows_setup_installs_and_uninstalls_silently() {
    let root = gof_conformance::workspace_root();
    let temp = tempdir().expect("tempdir should exist");
    let payload_dir = temp.path().join("payload");
    let output_path = temp.path().join("gof-setup.exe");
    let install_dir = temp.path().join("gof-install");
    fs::create_dir_all(&payload_dir).expect("payload dir should exist");

    fs::write(payload_dir.join("gof.exe"), b"fake-exe").expect("binary payload should be written");
    fs::write(payload_dir.join("README.md"), "readme").expect("readme payload should be written");
    fs::write(payload_dir.join("LICENSE"), "license").expect("license payload should be written");

    let build_script = root.join("scripts").join("build-windows-installer.ps1");

    Command::new("powershell")
        .current_dir(&root)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&build_script)
        .args(["-PayloadDir"])
        .arg(&payload_dir)
        .args(["-OutputPath"])
        .arg(&output_path)
        .args(["-Version", "0.0.0-test"])
        .assert()
        .success();

    Command::new(&output_path)
        .current_dir(&root)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .arg(format!("/DIR={}", install_dir.display()))
        .assert()
        .success();

    assert!(install_dir.join("bin").join("gof.exe").exists());
    assert!(install_dir.join("README.md").exists());
    assert!(install_dir.join("LICENSE").exists());
    assert_eq!(
        fs::read_to_string(install_dir.join("VERSION.txt")).expect("version file should exist"),
        "0.0.0-test\r\n"
    );

    let installed_bin = install_dir.join("bin").display().to_string();
    let user_path = read_user_path();
    assert!(
        user_path
            .split(';')
            .any(|segment| segment.eq_ignore_ascii_case(&installed_bin)),
        "installer should add the bin directory to the user PATH"
    );

    Command::new(install_dir.join("unins000.exe"))
        .current_dir(&root)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .assert()
        .success();

    thread::sleep(Duration::from_secs(2));
    assert!(!install_dir.join("bin").join("gof.exe").exists());
    let user_path_after_uninstall = read_user_path();
    assert!(
        user_path_after_uninstall
            .split(';')
            .all(|segment| !segment.eq_ignore_ascii_case(&installed_bin)),
        "uninstall should remove the bin directory from the user PATH"
    );
}
