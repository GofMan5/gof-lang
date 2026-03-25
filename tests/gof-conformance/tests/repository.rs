use std::path::Path;

#[test]
fn workspace_contains_governance_directories() {
    let root = gof_conformance::workspace_root();
    for path in [
        "spec",
        "rfcs",
        "adrs",
        "compiler",
        "runtime",
        "stdlib",
        "tools",
        "tests",
        "benchmarks",
    ] {
        assert!(Path::new(&root).join(path).exists(), "missing {path}");
    }
}
