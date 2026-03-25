pub fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests package should have a parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}
