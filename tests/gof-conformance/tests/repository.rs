use std::path::Path;

#[test]
fn workspace_contains_governance_directories() {
    let root = gof_conformance::workspace_root();
    for path in [
        "spec",
        "rfcs",
        "adrs",
        ".github",
        ".github/workflows",
        ".github/workflows/ci.yml",
        ".github/workflows/docs.yml",
        "docs",
        "docs/book",
        "docs/book/src",
        "docs/book/book.toml",
        "docs/book/src/SUMMARY.md",
        "docs/book/src/introduction.md",
        "docs/book/src/getting-started.md",
        "docs/book/src/language-tour.md",
        "docs/book/src/types-and-data.md",
        "docs/book/src/control-flow.md",
        "docs/book/src/modules-and-files.md",
        "docs/book/src/concurrency.md",
        "docs/book/src/stdlib-baseline.md",
        "docs/book/src/tooling-and-native-build.md",
        "docs/book/src/status-and-roadmap.md",
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
