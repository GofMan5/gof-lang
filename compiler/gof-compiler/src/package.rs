use pathdiff::diff_paths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

const LOCKFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageContext {
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub source_root: PathBuf,
    pub module_name: String,
    pub edition: String,
    pub dependencies: BTreeMap<String, LocalDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDependency {
    pub package_root: PathBuf,
    pub manifest_path: PathBuf,
    pub entry_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageLockfile {
    pub version: u32,
    pub root: LockfileRoot,
    #[serde(rename = "package")]
    pub packages: Vec<LockedPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfileRoot {
    pub module: String,
    pub edition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackage {
    pub module: String,
    pub identity: String,
    pub manifest_digest: String,
    pub dependencies: Vec<String>,
    pub entry: String,
    pub source: LockedPackageSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPackageSource {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Error)]
pub enum PackageManifestError {
    #[error("failed to read manifest `{path}`: {message}")]
    Read { path: PathBuf, message: String },
    #[error("failed to parse manifest `{path}`: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(
        "dependency `{dependency}` declared in `{manifest}` is missing a manifest at `{expected_manifest}`"
    )]
    MissingDependencyManifest {
        manifest: PathBuf,
        dependency: String,
        expected_manifest: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum PackageGraphError {
    #[error(transparent)]
    Manifest(#[from] PackageManifestError),
    #[error(
        "dependency cycle detected in the local package graph: {cycle}",
        cycle = format_cycle(.cycle)
    )]
    DependencyCycle { cycle: Vec<PathBuf> },
    #[error(
        "module `{module}` resolves to multiple package roots: `{existing_root}` and `{new_root}`"
    )]
    ConflictingModuleIdentity {
        module: String,
        existing_root: PathBuf,
        new_root: PathBuf,
    },
    #[error(
        "package root `{package_root}` resolved with conflicting metadata: expected `{existing_module}` edition `{existing_edition}`, got `{new_module}` edition `{new_edition}`"
    )]
    ConflictingPackageMetadata {
        package_root: PathBuf,
        existing_module: String,
        existing_edition: String,
        new_module: String,
        new_edition: String,
    },
    #[error(
        "package `{module}` at `{package_root}` is missing the required entrypoint `{expected_entry}`"
    )]
    MissingEntrypoint {
        module: String,
        package_root: PathBuf,
        expected_entry: PathBuf,
    },
    #[error("package root `{start}` does not contain a `gof.mod` manifest")]
    MissingRootManifest { start: PathBuf },
    #[error("failed to read lockfile `{path}`: {message}")]
    LockfileRead { path: PathBuf, message: String },
    #[error("failed to write lockfile `{path}`: {message}")]
    LockfileWrite { path: PathBuf, message: String },
    #[error("failed to parse lockfile `{path}`: {message}")]
    LockfileParse { path: PathBuf, message: String },
    #[error("manifest-backed package `{module}` is missing `{path}`")]
    MissingLockfile {
        module: String,
        package_root: PathBuf,
        path: PathBuf,
    },
    #[error("lockfile `{path}` is stale for manifest-backed package `{module}`: {reason}")]
    StaleLockfile {
        module: String,
        package_root: PathBuf,
        path: PathBuf,
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
struct RawPackageManifest {
    module: String,
    edition: String,
    #[serde(default)]
    dependencies: HashMap<String, RawDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawDependency {
    Path(String),
    Detail { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadedPackageManifest {
    manifest_path: PathBuf,
    package_root: PathBuf,
    source_root: PathBuf,
    module: String,
    edition: String,
    dependencies: BTreeMap<String, ManifestDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestDependency {
    resolved_root: PathBuf,
    normalized_from_package: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedPackage {
    module: String,
    edition: String,
    package_root: PathBuf,
    manifest_digest: String,
    identity: String,
    source_path: String,
    dependencies: Vec<String>,
    entry: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageRole {
    Root,
    Dependency,
}

impl RawDependency {
    fn path(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Detail { path } => path,
        }
    }
}

pub fn find_package_context_for_source(
    source_path: &Path,
) -> Result<Option<PackageContext>, PackageManifestError> {
    let Some(manifest_path) = find_package_manifest_path(source_path) else {
        return Ok(None);
    };
    load_package_context(&manifest_path).map(Some)
}

pub fn find_package_root(start: &Path) -> Option<PathBuf> {
    find_package_manifest_path(start).and_then(|manifest_path| {
        manifest_path
            .parent()
            .map(Path::to_path_buf)
            .map(|path| normalize_path(&path))
    })
}

pub fn package_main_entry_path(package_root: &Path) -> PathBuf {
    package_root.join("src").join("main.gof")
}

pub fn package_library_entry_path(package_root: &Path) -> PathBuf {
    package_root.join("src").join("lib.gof")
}

pub fn resolve_lockfile_for_directory(
    directory: &Path,
) -> Result<(PathBuf, PackageLockfile), PackageGraphError> {
    let root_manifest_path = find_package_manifest_path(directory).ok_or_else(|| {
        PackageGraphError::MissingRootManifest {
            start: normalize_path(directory),
        }
    })?;
    let graph = resolve_package_graph(&root_manifest_path)?;
    let package_root = root_manifest_path
        .parent()
        .expect("package manifest should always have a parent");
    Ok((package_root.join("gof.lock"), graph))
}

pub fn write_lockfile_for_directory(directory: &Path) -> Result<PathBuf, PackageGraphError> {
    let (lockfile_path, lockfile) = resolve_lockfile_for_directory(directory)?;
    let serialized = serialize_lockfile(&lockfile)?;
    std::fs::write(&lockfile_path, serialized).map_err(|error| {
        PackageGraphError::LockfileWrite {
            path: lockfile_path.clone(),
            message: error.to_string(),
        }
    })?;
    Ok(lockfile_path)
}

pub fn ensure_fresh_lockfile_for_source(
    source_path: &Path,
) -> Result<Option<PackageLockfile>, PackageGraphError> {
    let Some(package_context) = find_package_context_for_source(source_path)? else {
        return Ok(None);
    };

    let (lockfile_path, expected_lockfile) =
        resolve_lockfile_for_directory(&package_context.package_root)?;
    if !lockfile_path.is_file() {
        return Err(PackageGraphError::MissingLockfile {
            module: package_context.module_name,
            package_root: package_context.package_root,
            path: lockfile_path,
        });
    }

    let actual_text = std::fs::read_to_string(&lockfile_path).map_err(|error| {
        PackageGraphError::LockfileRead {
            path: lockfile_path.clone(),
            message: error.to_string(),
        }
    })?;
    let actual_lockfile: PackageLockfile =
        toml::from_str(&actual_text).map_err(|error| PackageGraphError::LockfileParse {
            path: lockfile_path.clone(),
            message: error.to_string(),
        })?;

    if actual_lockfile != expected_lockfile {
        return Err(PackageGraphError::StaleLockfile {
            module: package_context.module_name,
            package_root: package_context.package_root,
            path: lockfile_path,
            reason: describe_lockfile_mismatch(&expected_lockfile, &actual_lockfile),
        });
    }

    Ok(Some(actual_lockfile))
}

fn load_package_context(manifest_path: &Path) -> Result<PackageContext, PackageManifestError> {
    let manifest = load_package_manifest(manifest_path)?;
    let mut dependencies = BTreeMap::new();

    for (name, dependency) in &manifest.dependencies {
        let dependency_manifest = dependency.resolved_root.join("gof.mod");
        if !dependency_manifest.is_file() {
            return Err(PackageManifestError::MissingDependencyManifest {
                manifest: manifest.manifest_path.clone(),
                dependency: name.clone(),
                expected_manifest: dependency_manifest,
            });
        }

        dependencies.insert(
            name.clone(),
            LocalDependency {
                entry_path: package_library_entry_path(&dependency.resolved_root),
                manifest_path: dependency_manifest,
                package_root: dependency.resolved_root.clone(),
            },
        );
    }

    Ok(PackageContext {
        dependencies,
        edition: manifest.edition,
        manifest_path: manifest.manifest_path,
        module_name: manifest.module,
        package_root: manifest.package_root.clone(),
        source_root: manifest.source_root,
    })
}

fn resolve_package_graph(root_manifest_path: &Path) -> Result<PackageLockfile, PackageGraphError> {
    let root_manifest = load_package_manifest(root_manifest_path)?;
    let root_root = root_manifest.package_root.clone();
    let mut builder = GraphBuilder::new(root_root);
    builder.visit(root_manifest, PackageRole::Root)?;

    let mut packages = builder
        .packages
        .into_values()
        .map(|package| LockedPackage {
            module: package.module,
            identity: package.identity,
            manifest_digest: package.manifest_digest,
            dependencies: package.dependencies,
            entry: package.entry,
            source: LockedPackageSource {
                kind: "path".to_string(),
                path: package.source_path,
            },
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| {
        left.source
            .path
            .cmp(&right.source.path)
            .then_with(|| left.module.cmp(&right.module))
    });

    let root_package = packages
        .iter()
        .find(|package| package.source.path == ".")
        .expect("resolved graph must always contain the root package");
    Ok(PackageLockfile {
        version: LOCKFILE_VERSION,
        root: LockfileRoot {
            module: root_package.module.clone(),
            edition: builder.root_edition,
        },
        packages,
    })
}

struct GraphBuilder {
    root_package_root: PathBuf,
    root_edition: String,
    packages: HashMap<PathBuf, ResolvedPackage>,
    modules: HashMap<String, PathBuf>,
    stack: Vec<PathBuf>,
}

impl GraphBuilder {
    fn new(root_package_root: PathBuf) -> Self {
        Self {
            root_package_root,
            root_edition: String::new(),
            packages: HashMap::new(),
            modules: HashMap::new(),
            stack: Vec::new(),
        }
    }

    fn visit(
        &mut self,
        manifest: LoadedPackageManifest,
        role: PackageRole,
    ) -> Result<String, PackageGraphError> {
        let package_root = manifest.package_root.clone();
        if let Some(position) = self
            .stack
            .iter()
            .position(|candidate| *candidate == package_root)
        {
            let mut cycle = self.stack[position..].to_vec();
            cycle.push(package_root.clone());
            return Err(PackageGraphError::DependencyCycle { cycle });
        }

        if let Some(existing_root) = self.modules.get(&manifest.module) {
            if *existing_root != package_root {
                return Err(PackageGraphError::ConflictingModuleIdentity {
                    module: manifest.module.clone(),
                    existing_root: existing_root.clone(),
                    new_root: package_root,
                });
            }
        }

        match self.packages.entry(manifest.package_root.clone()) {
            Entry::Occupied(existing) => {
                let existing = existing.get();
                if existing.module != manifest.module || existing.edition != manifest.edition {
                    return Err(PackageGraphError::ConflictingPackageMetadata {
                        package_root: existing.package_root.clone(),
                        existing_module: existing.module.clone(),
                        existing_edition: existing.edition.clone(),
                        new_module: manifest.module,
                        new_edition: manifest.edition,
                    });
                }
                return Ok(existing.module.clone());
            }
            Entry::Vacant(_) => {}
        }

        if role == PackageRole::Root {
            self.root_edition = manifest.edition.clone();
        }

        self.modules
            .insert(manifest.module.clone(), manifest.package_root.clone());
        self.stack.push(manifest.package_root.clone());

        let mut dependency_modules = Vec::new();
        for (alias, dependency) in &manifest.dependencies {
            let dependency_manifest_path = dependency.resolved_root.join("gof.mod");
            if !dependency_manifest_path.is_file() {
                return Err(PackageManifestError::MissingDependencyManifest {
                    manifest: manifest.manifest_path.clone(),
                    dependency: alias.clone(),
                    expected_manifest: dependency_manifest_path,
                }
                .into());
            }

            let dependency_manifest = load_package_manifest(&dependency_manifest_path)?;
            let dependency_module = self.visit(dependency_manifest, PackageRole::Dependency)?;
            dependency_modules.push(dependency_module);
        }
        dependency_modules.sort();

        let entry_path = match role {
            PackageRole::Root => preferred_root_entry(&manifest).ok_or_else(|| {
                PackageGraphError::MissingEntrypoint {
                    module: manifest.module.clone(),
                    package_root: manifest.package_root.clone(),
                    expected_entry: package_main_entry_path(&manifest.package_root),
                }
            })?,
            PackageRole::Dependency => {
                let entry_path = package_library_entry_path(&manifest.package_root);
                if !entry_path.is_file() {
                    return Err(PackageGraphError::MissingEntrypoint {
                        module: manifest.module.clone(),
                        package_root: manifest.package_root.clone(),
                        expected_entry: entry_path,
                    });
                }
                entry_path
            }
        };

        let source_path =
            normalize_relative_path_to(&self.root_package_root, &manifest.package_root);
        let manifest_digest = manifest_digest(&manifest);
        let identity = package_identity(&manifest.module, &source_path);
        let entry = normalize_relative_path_to(&manifest.package_root, &entry_path);
        let resolved = ResolvedPackage {
            module: manifest.module.clone(),
            edition: manifest.edition.clone(),
            package_root: manifest.package_root.clone(),
            manifest_digest,
            identity,
            source_path,
            dependencies: dependency_modules,
            entry,
        };
        self.packages
            .insert(manifest.package_root.clone(), resolved);
        self.stack.pop();

        Ok(manifest.module)
    }
}

fn preferred_root_entry(manifest: &LoadedPackageManifest) -> Option<PathBuf> {
    let main_path = package_main_entry_path(&manifest.package_root);
    if main_path.is_file() {
        return Some(main_path);
    }

    let lib_path = package_library_entry_path(&manifest.package_root);
    lib_path.is_file().then_some(lib_path)
}

fn load_package_manifest(
    manifest_path: &Path,
) -> Result<LoadedPackageManifest, PackageManifestError> {
    let manifest_path = normalize_path(manifest_path);
    let manifest_text =
        std::fs::read_to_string(&manifest_path).map_err(|error| PackageManifestError::Read {
            path: manifest_path.clone(),
            message: error.to_string(),
        })?;
    let manifest: RawPackageManifest =
        toml::from_str(&manifest_text).map_err(|error| PackageManifestError::Parse {
            path: manifest_path.clone(),
            message: error.to_string(),
        })?;

    let package_root = manifest_path
        .parent()
        .expect("manifest should always have a parent directory")
        .to_path_buf();
    let source_root = package_root.join("src");
    let mut dependencies = BTreeMap::new();

    for (alias, dependency) in manifest.dependencies {
        let declared_root = PathBuf::from(dependency.path());
        let resolved_root = if declared_root.is_absolute() {
            declared_root
        } else {
            package_root.join(declared_root)
        };
        let resolved_root = normalize_path(&resolved_root);
        dependencies.insert(
            alias,
            ManifestDependency {
                normalized_from_package: normalize_relative_path_to(&package_root, &resolved_root),
                resolved_root,
            },
        );
    }

    Ok(LoadedPackageManifest {
        manifest_path,
        package_root,
        source_root,
        module: manifest.module,
        edition: manifest.edition,
        dependencies,
    })
}

fn find_package_manifest_path(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };

    while let Some(directory) = current {
        let manifest_path = directory.join("gof.mod");
        if manifest_path.is_file() {
            return Some(normalize_path(&manifest_path));
        }
        current = directory.parent();
    }

    None
}

fn normalize_path(path: &Path) -> PathBuf {
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

fn normalize_relative_path_to(base: &Path, target: &Path) -> String {
    let normalized = diff_paths(target, base)
        .unwrap_or_else(|| target.to_path_buf())
        .components()
        .filter_map(normalize_component)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized.join("/")
    }
}

fn normalize_component(component: Component<'_>) -> Option<String> {
    match component {
        Component::CurDir => None,
        Component::ParentDir => Some("..".to_string()),
        Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
        Component::RootDir => Some(String::new()),
        Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().replace('\\', "/")),
    }
}

fn manifest_digest(manifest: &LoadedPackageManifest) -> String {
    let mut buffer = String::new();
    let _ = writeln!(buffer, "module={}", manifest.module);
    let _ = writeln!(buffer, "edition={}", manifest.edition);
    for (alias, dependency) in &manifest.dependencies {
        let _ = writeln!(buffer, "dep.{alias}={}", dependency.normalized_from_package);
    }
    digest_string(&buffer)
}

fn package_identity(module: &str, root_relative_path: &str) -> String {
    format!("{module}@{}", digest_string(root_relative_path))
}

fn digest_string(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn serialize_lockfile(lockfile: &PackageLockfile) -> Result<String, PackageGraphError> {
    let mut rendered =
        toml::to_string_pretty(lockfile).map_err(|error| PackageGraphError::LockfileParse {
            path: PathBuf::from("gof.lock"),
            message: error.to_string(),
        })?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

fn describe_lockfile_mismatch(expected: &PackageLockfile, actual: &PackageLockfile) -> String {
    if actual.version != expected.version {
        return format!(
            "lockfile version changed from {} to {}",
            actual.version, expected.version
        );
    }
    if actual.root != expected.root {
        return format!(
            "root package metadata changed from {:?} to {:?}",
            actual.root, expected.root
        );
    }
    if actual.packages.len() != expected.packages.len() {
        return format!(
            "package graph size changed from {} package(s) to {} package(s)",
            actual.packages.len(),
            expected.packages.len()
        );
    }

    for (expected_package, actual_package) in expected.packages.iter().zip(&actual.packages) {
        if actual_package.module != expected_package.module {
            return format!("package order changed for `{}`", expected_package.module);
        }
        if actual_package.identity != expected_package.identity {
            return format!("package identity changed for `{}`", expected_package.module);
        }
        if actual_package.source != expected_package.source {
            return format!(
                "package source path changed for `{}`",
                expected_package.module
            );
        }
        if actual_package.manifest_digest != expected_package.manifest_digest {
            return format!("manifest changed for `{}`", expected_package.module);
        }
        if actual_package.dependencies != expected_package.dependencies {
            return format!("dependency graph changed for `{}`", expected_package.module);
        }
        if actual_package.entry != expected_package.entry {
            return format!("entrypoint changed for `{}`", expected_package.module);
        }
    }

    "manifest-backed package graph changed".to_string()
}

fn format_cycle(cycle: &[PathBuf]) -> String {
    cycle
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::{
        PackageGraphError, ensure_fresh_lockfile_for_source, normalize_relative_path_to,
        resolve_lockfile_for_directory, write_lockfile_for_directory,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn lockfile_round_trips_with_stable_ordering() {
        let temp = tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let app_root = temp.path().join("package_app");
        write_local_package_pair(&app_root, &math_root);

        let (_, first) = resolve_lockfile_for_directory(&app_root).expect("lock should resolve");
        let (_, second) = resolve_lockfile_for_directory(&app_root).expect("lock should resolve");

        assert_eq!(first, second);
        let rendered = toml::to_string_pretty(&first).expect("lock should serialize");
        let parsed = toml::from_str(&rendered).expect("lock should parse");
        assert_eq!(first, parsed);
        assert_eq!(first.packages[0].source.path, ".");
        assert_eq!(first.packages[1].source.path, "../package_math");
        assert_eq!(
            first
                .packages
                .iter()
                .map(|package| package.module.clone())
                .collect::<Vec<_>>(),
            vec![
                "example/package_app".to_string(),
                "example/package_math".to_string()
            ]
        );
    }

    #[test]
    fn relative_path_normalization_uses_forward_slashes() {
        let base = Path::new("E:\\repo\\package_app");
        let target = Path::new("E:\\repo\\package_math\\src\\lib.gof");
        assert_eq!(
            normalize_relative_path_to(base, target),
            "../package_math/src/lib.gof"
        );
    }

    #[test]
    fn source_edits_do_not_stale_the_lockfile() {
        let temp = tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let app_root = temp.path().join("package_app");
        write_local_package_pair(&app_root, &math_root);
        write_lockfile_for_directory(&app_root).expect("lock should be written");

        fs::write(
            app_root.join("src").join("main.gof"),
            "import package_math\n\nfn main() -> int:\n    return square(10)\n",
        )
        .expect("main source should be updated");

        let validation = ensure_fresh_lockfile_for_source(&app_root.join("src").join("main.gof"));
        assert!(
            validation.is_ok(),
            "source-only edits must not stale the lockfile"
        );
    }

    #[test]
    fn manifest_edits_do_stale_the_lockfile() {
        let temp = tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let app_root = temp.path().join("package_app");
        let extra_root = temp.path().join("package_extra");
        write_local_package_pair(&app_root, &math_root);
        fs::create_dir_all(extra_root.join("src")).expect("extra source root should exist");
        fs::write(
            extra_root.join("gof.mod"),
            "module = \"example/package_extra\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("extra manifest should exist");
        fs::write(
            extra_root.join("src").join("lib.gof"),
            "fn bonus(value: int) -> int:\n    return value + 1\n",
        )
        .expect("extra library should exist");

        write_lockfile_for_directory(&app_root).expect("lock should be written");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\npackage_extra = { path = \"../package_extra\" }\n",
        )
        .expect("manifest should be updated");

        let error = ensure_fresh_lockfile_for_source(&app_root.join("src").join("main.gof"))
            .expect_err("manifest edit must stale the lock");
        assert!(matches!(error, PackageGraphError::StaleLockfile { .. }));
    }

    #[test]
    fn conflicting_module_roots_are_rejected() {
        let temp = tempdir().expect("tempdir should exist");
        let alpha_root = temp.path().join("package_alpha");
        let beta_root = temp.path().join("package_beta");
        let app_root = temp.path().join("package_app");

        fs::create_dir_all(alpha_root.join("src")).expect("alpha source root should exist");
        fs::create_dir_all(beta_root.join("src")).expect("beta source root should exist");
        fs::create_dir_all(app_root.join("src")).expect("app source root should exist");
        fs::write(
            alpha_root.join("gof.mod"),
            "module = \"example/shared\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("alpha manifest should exist");
        fs::write(
            alpha_root.join("src").join("lib.gof"),
            "fn alpha() -> int:\n    return 1\n",
        )
        .expect("alpha lib should exist");
        fs::write(
            beta_root.join("gof.mod"),
            "module = \"example/shared\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("beta manifest should exist");
        fs::write(
            beta_root.join("src").join("lib.gof"),
            "fn beta() -> int:\n    return 2\n",
        )
        .expect("beta lib should exist");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\na = { path = \"../package_alpha\" }\nb = { path = \"../package_beta\" }\n",
        )
        .expect("app manifest should exist");
        fs::write(
            app_root.join("src").join("main.gof"),
            "import a\nimport b\n\nfn main() -> int:\n    return 0\n",
        )
        .expect("main source should exist");

        let error = resolve_lockfile_for_directory(&app_root)
            .expect_err("conflicting modules must be rejected");
        assert!(matches!(
            error,
            PackageGraphError::ConflictingModuleIdentity { .. }
        ));
    }

    #[test]
    fn package_cycles_are_rejected_deterministically() {
        let temp = tempdir().expect("tempdir should exist");
        let alpha_root = temp.path().join("package_alpha");
        let beta_root = temp.path().join("package_beta");

        fs::create_dir_all(alpha_root.join("src")).expect("alpha source root should exist");
        fs::create_dir_all(beta_root.join("src")).expect("beta source root should exist");
        fs::write(
            alpha_root.join("gof.mod"),
            "module = \"example/package_alpha\"\nedition = \"2026\"\n\n[dependencies]\npackage_beta = { path = \"../package_beta\" }\n",
        )
        .expect("alpha manifest should exist");
        fs::write(
            alpha_root.join("src").join("main.gof"),
            "fn main() -> int:\n    return 0\n",
        )
        .expect("alpha main should exist");
        fs::write(
            beta_root.join("gof.mod"),
            "module = \"example/package_beta\"\nedition = \"2026\"\n\n[dependencies]\npackage_alpha = { path = \"../package_alpha\" }\n",
        )
        .expect("beta manifest should exist");
        fs::write(
            beta_root.join("src").join("lib.gof"),
            "fn ping() -> int:\n    return 1\n",
        )
        .expect("beta lib should exist");

        let first =
            resolve_lockfile_for_directory(&alpha_root).expect_err("dependency cycle should fail");
        let second =
            resolve_lockfile_for_directory(&alpha_root).expect_err("dependency cycle should fail");
        assert_eq!(first.to_string(), second.to_string());
        assert!(matches!(first, PackageGraphError::DependencyCycle { .. }));
    }

    #[test]
    fn moved_dependency_path_invalidates_the_lock() {
        let temp = tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let moved_math_root = temp.path().join("package_math_renamed");
        let app_root = temp.path().join("package_app");
        write_local_package_pair(&app_root, &math_root);
        write_lockfile_for_directory(&app_root).expect("lock should be written");

        fs::rename(&math_root, &moved_math_root).expect("dependency should be moved");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math_renamed\" }\n",
        )
        .expect("manifest should be updated");

        let error = ensure_fresh_lockfile_for_source(&app_root.join("src").join("main.gof"))
            .expect_err("moved dependency should stale the lock");
        assert!(matches!(error, PackageGraphError::StaleLockfile { .. }));
    }

    fn write_local_package_pair(app_root: &Path, math_root: &Path) {
        fs::create_dir_all(app_root.join("src")).expect("app source root should exist");
        fs::create_dir_all(math_root.join("src")).expect("math source root should exist");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
        )
        .expect("app manifest should exist");
        fs::write(
            app_root.join("src").join("main.gof"),
            "import package_math\n\nfn main() -> int:\n    return square(9)\n",
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
    }
}
