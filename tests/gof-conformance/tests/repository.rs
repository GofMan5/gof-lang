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
        "docs/site",
        "docs/site/app",
        "docs/site/content",
        "docs/site/content/docs",
        "docs/site/content/docs/en",
        "docs/site/content/docs/en/meta.json",
        "docs/site/content/docs/en/index.mdx",
        "docs/site/content/docs/en/getting-started.mdx",
        "docs/site/content/docs/en/language-tour.mdx",
        "docs/site/content/docs/en/types-and-data.mdx",
        "docs/site/content/docs/en/control-flow.mdx",
        "docs/site/content/docs/en/modules-and-files.mdx",
        "docs/site/content/docs/en/concurrency.mdx",
        "docs/site/content/docs/en/stdlib-baseline.mdx",
        "docs/site/content/docs/en/testing.mdx",
        "docs/site/content/docs/en/tooling-and-native-build.mdx",
        "docs/site/content/docs/en/status-and-roadmap.mdx",
        "docs/site/content/docs/ru",
        "docs/site/content/docs/ru/meta.json",
        "docs/site/content/docs/ru/index.mdx",
        "docs/site/content/docs/ru/getting-started.mdx",
        "docs/site/content/docs/ru/language-tour.mdx",
        "docs/site/content/docs/ru/types-and-data.mdx",
        "docs/site/content/docs/ru/control-flow.mdx",
        "docs/site/content/docs/ru/modules-and-files.mdx",
        "docs/site/content/docs/ru/concurrency.mdx",
        "docs/site/content/docs/ru/stdlib-baseline.mdx",
        "docs/site/content/docs/ru/testing.mdx",
        "docs/site/content/docs/ru/tooling-and-native-build.mdx",
        "docs/site/content/docs/ru/status-and-roadmap.mdx",
        "docs/site/package.json",
        "docs/site/source.config.ts",
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

    let getting_started = read("docs/site/content/docs/en/getting-started.mdx");
    assert!(
        getting_started.contains("gof run hello.gof"),
        "English getting-started guide must teach installed gof usage first"
    );
    assert!(
        getting_started.contains("cargo run -q -p gof-cli --bin gof --"),
        "English getting-started guide must keep a developer fallback"
    );

    let getting_started_ru = read("docs/site/content/docs/ru/getting-started.mdx");
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
