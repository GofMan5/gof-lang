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
        "docs/book/theme",
        "docs/book/theme/custom.css",
        "docs/book/theme/custom.js",
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
        "docs/book-ru",
        "docs/book-ru/src",
        "docs/book-ru/book.toml",
        "docs/book-ru/src/SUMMARY.md",
        "docs/book-ru/src/introduction.md",
        "docs/book-ru/src/getting-started.md",
        "docs/book-ru/src/language-tour.md",
        "docs/book-ru/src/types-and-data.md",
        "docs/book-ru/src/control-flow.md",
        "docs/book-ru/src/modules-and-files.md",
        "docs/book-ru/src/concurrency.md",
        "docs/book-ru/src/stdlib-baseline.md",
        "docs/book-ru/src/tooling-and-native-build.md",
        "docs/book-ru/src/status-and-roadmap.md",
        "compiler",
        "runtime",
        "stdlib",
        "tools",
        "tests",
        "tests/fixtures/runtime-fail",
        "benchmarks",
    ] {
        assert!(Path::new(&root).join(path).exists(), "missing {path}");
    }
}

#[test]
fn public_docs_prefer_installed_gof_cli() {
    let root = gof_conformance::workspace_root();
    let read = |path: &str| std::fs::read_to_string(Path::new(&root).join(path)).unwrap();

    let readme = read("README.md");
    assert!(
        readme.contains("Rust is not required to run `gof` programs."),
        "README must explain that normal usage does not require Rust"
    );
    assert!(
        readme.contains("gof run examples/geometry.gof"),
        "README must show installed gof usage"
    );

    let getting_started = read("docs/book/src/getting-started.md");
    assert!(
        getting_started.contains("gof run hello.gof"),
        "English getting-started guide must teach installed gof usage first"
    );
    assert!(
        getting_started.contains("cargo run -q -p gof-cli --bin gof --"),
        "English getting-started guide must keep a developer fallback"
    );

    let getting_started_ru = read("docs/book-ru/src/getting-started.md");
    assert!(
        getting_started_ru.contains("gof run hello.gof"),
        "Russian getting-started guide must teach installed gof usage first"
    );
    assert!(
        getting_started_ru.contains("cargo run -q -p gof-cli --bin gof --"),
        "Russian getting-started guide must keep a developer fallback"
    );

    let examples = read("examples/README.md");
    assert!(
        examples.contains("gof run <example>"),
        "examples README must show the installed CLI path"
    );
}
