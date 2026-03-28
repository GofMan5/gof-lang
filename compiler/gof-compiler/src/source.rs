use crate::package::{PackageContext, PackageManifestError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const RESERVED_STDLIB_MODULE_NAMES: &[&str] = &["bytes", "io", "time", "net", "http", "testing"];

fn compiler_workspace_root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .expect("compiler crate should live under the workspace root")
    })
}

pub fn is_reserved_stdlib_module_name(module_name: &str) -> bool {
    RESERVED_STDLIB_MODULE_NAMES.contains(&module_name)
}

pub fn stdlib_module_path(module_name: &str) -> Option<PathBuf> {
    is_reserved_stdlib_module_name(module_name).then(|| {
        normalize_source_path(
            &compiler_workspace_root()
                .join("stdlib")
                .join(format!("{module_name}.gof")),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    path: PathBuf,
    text: String,
}

impl SourceFile {
    pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
        }
    }

    pub fn from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        Ok(Self::new(
            path.to_path_buf(),
            std::fs::read_to_string(path)?,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

pub trait SourceProvider {
    fn normalize_path(&self, path: &Path) -> PathBuf;
    fn is_file(&self, path: &Path) -> bool;
    fn load_source(&self, path: &Path) -> io::Result<SourceFile>;
    fn stdlib_module_path(&self, module_name: &str) -> Option<PathBuf>;
    fn package_context_for_source(
        &self,
        source_path: &Path,
    ) -> Result<Option<PackageContext>, PackageManifestError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FileSystemSourceProvider;

impl SourceProvider for FileSystemSourceProvider {
    fn normalize_path(&self, path: &Path) -> PathBuf {
        normalize_source_path(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.normalize_path(path).is_file()
    }

    fn load_source(&self, path: &Path) -> io::Result<SourceFile> {
        let normalized_path = self.normalize_path(path);
        Ok(SourceFile::new(
            normalized_path.clone(),
            std::fs::read_to_string(&normalized_path)?,
        ))
    }

    fn stdlib_module_path(&self, module_name: &str) -> Option<PathBuf> {
        stdlib_module_path(module_name)
    }

    fn package_context_for_source(
        &self,
        source_path: &Path,
    ) -> Result<Option<PackageContext>, PackageManifestError> {
        crate::package::find_package_context_for_source(source_path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedSourceBundle {
    pub entry_path: PathBuf,
    pub sources: Vec<SourceFile>,
    pub package_contexts: Vec<EmbeddedPackageContext>,
}

impl EmbeddedSourceBundle {
    pub fn entry_source(&self) -> io::Result<SourceFile> {
        self.sources
            .iter()
            .find(|source| source.path() == self.entry_path.as_path())
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "embedded source bundle is missing entry source `{}`",
                        self.entry_path.display()
                    ),
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedPackageContext {
    pub source_path: PathBuf,
    pub context: Option<PackageContext>,
}

#[derive(Debug, Clone)]
pub struct EmbeddedSourceProvider {
    sources: HashMap<PathBuf, SourceFile>,
    package_contexts: HashMap<PathBuf, Option<PackageContext>>,
}

impl EmbeddedSourceProvider {
    pub fn new(bundle: EmbeddedSourceBundle) -> Self {
        let sources = bundle
            .sources
            .into_iter()
            .map(|source| (normalize_source_path(source.path()), source))
            .collect();
        let package_contexts = bundle
            .package_contexts
            .into_iter()
            .map(|entry| (normalize_source_path(&entry.source_path), entry.context))
            .collect();
        Self {
            sources,
            package_contexts,
        }
    }
}

impl SourceProvider for EmbeddedSourceProvider {
    fn normalize_path(&self, path: &Path) -> PathBuf {
        normalize_source_path(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.sources.contains_key(&self.normalize_path(path))
    }

    fn load_source(&self, path: &Path) -> io::Result<SourceFile> {
        self.sources
            .get(&self.normalize_path(path))
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("embedded bundle does not contain `{}`", path.display()),
                )
            })
    }

    fn stdlib_module_path(&self, module_name: &str) -> Option<PathBuf> {
        stdlib_module_path(module_name)
    }

    fn package_context_for_source(
        &self,
        source_path: &Path,
    ) -> Result<Option<PackageContext>, PackageManifestError> {
        Ok(self
            .package_contexts
            .get(&self.normalize_path(source_path))
            .cloned()
            .flatten())
    }
}

pub fn normalize_source_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub end_column: usize,
}

impl Span {
    pub const fn new(line: usize, column: usize, end_column: usize) -> Self {
        Self {
            line,
            column,
            end_column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EmbeddedPackageContext, EmbeddedSourceBundle, EmbeddedSourceProvider,
        FileSystemSourceProvider, SourceFile, SourceProvider, is_reserved_stdlib_module_name,
        stdlib_module_path,
    };
    use crate::package::{LocalDependency, PackageContext};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn embedded_source_bundle_returns_its_entry_source() {
        let entry_path = PathBuf::from("/bundle/main.gof");
        let bundle = EmbeddedSourceBundle {
            entry_path: entry_path.clone(),
            sources: vec![
                SourceFile::new("/bundle/helper.gof", "fn helper() -> int:\n    return 1\n"),
                SourceFile::new(&entry_path, "fn main() -> int:\n    return 42\n"),
            ],
            package_contexts: Vec::new(),
        };

        let entry = bundle
            .entry_source()
            .expect("bundle should expose entry source");
        assert_eq!(entry.path(), entry_path.as_path());
        assert!(entry.text().contains("return 42"));
    }

    #[test]
    fn embedded_source_provider_serves_sources_and_package_contexts() {
        let entry_path = PathBuf::from("/bundle/main.gof");
        let context = PackageContext {
            manifest_path: PathBuf::from("/bundle/gof.mod"),
            package_root: PathBuf::from("/bundle"),
            source_root: PathBuf::from("/bundle/src"),
            module_name: "example/app".to_string(),
            edition: "2026".to_string(),
            dependencies: BTreeMap::from([(
                "math".to_string(),
                LocalDependency {
                    package_root: PathBuf::from("/deps/math"),
                    manifest_path: PathBuf::from("/deps/math/gof.mod"),
                    entry_path: PathBuf::from("/deps/math/src/lib.gof"),
                },
            )]),
        };
        let provider = EmbeddedSourceProvider::new(EmbeddedSourceBundle {
            entry_path: entry_path.clone(),
            sources: vec![SourceFile::new(
                &entry_path,
                "fn main() -> int:\n    return 42\n",
            )],
            package_contexts: vec![EmbeddedPackageContext {
                source_path: entry_path.clone(),
                context: Some(context.clone()),
            }],
        });

        let loaded = provider
            .load_source(&entry_path)
            .expect("provider should load embedded source");
        assert_eq!(loaded.path(), entry_path.as_path());
        assert!(provider.is_file(&entry_path));
        assert_eq!(
            provider
                .package_context_for_source(&entry_path)
                .expect("package context lookup should succeed"),
            Some(context)
        );
    }

    #[test]
    fn filesystem_provider_normalizes_relative_paths() {
        let provider = FileSystemSourceProvider;
        let normalized = provider.normalize_path(PathBuf::from("relative-main.gof").as_path());
        assert!(normalized.is_absolute());
    }

    #[test]
    fn stdlib_module_names_are_reserved_and_mapped_into_workspace_stdlib() {
        assert!(is_reserved_stdlib_module_name("http"));
        assert!(is_reserved_stdlib_module_name("bytes"));
        assert!(is_reserved_stdlib_module_name("testing"));
        assert!(!is_reserved_stdlib_module_name("math"));

        let http_path = stdlib_module_path("http").expect("http should resolve to stdlib");
        assert!(http_path.ends_with(Path::new("stdlib").join("http.gof")));
        assert!(
            stdlib_module_path("math").is_none(),
            "non-stdlib names should not resolve into stdlib",
        );
    }

    #[test]
    fn embedded_source_provider_reuses_workspace_stdlib_paths() {
        let http_path = stdlib_module_path("http").expect("http should resolve to stdlib");
        let provider = EmbeddedSourceProvider::new(EmbeddedSourceBundle {
            entry_path: PathBuf::from("/bundle/main.gof"),
            sources: vec![SourceFile::new(&http_path, "\n")],
            package_contexts: Vec::new(),
        });

        assert_eq!(
            provider.stdlib_module_path("http"),
            Some(http_path),
            "embedded providers should keep the same reserved stdlib path contract",
        );
    }
}
