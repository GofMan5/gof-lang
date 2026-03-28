use crate::ast::{Module, parse};
use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::lexer::lex;
use crate::package::{PackageContext, PackageManifestError};
use crate::source::{FileSystemSourceProvider, SourceFile, SourceProvider};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedModuleGraph {
    pub module: Module,
    pub sources: Vec<SourceFile>,
}

#[derive(Debug)]
struct ModuleResolver<'provider, Provider> {
    provider: &'provider Provider,
    modules: HashMap<PathBuf, Module>,
    sources: HashMap<PathBuf, SourceFile>,
    package_contexts: HashMap<PathBuf, Option<PackageContext>>,
    load_order: Vec<PathBuf>,
    loading_stack: Vec<PathBuf>,
}

#[derive(Debug)]
enum ImportResolutionError {
    Manifest(PackageManifestError),
    ReservedStdlibConflict {
        stdlib_path: PathBuf,
        conflicts: Vec<String>,
    },
    Unresolved {
        searched: Vec<PathBuf>,
    },
}

pub fn parse_single_source(source: &SourceFile) -> Result<Module, Diagnostics> {
    let tokens = lex(source).map_err(|diagnostics| diagnostics.with_source_path(source.path()))?;
    let cst = CstModule::new(tokens);
    let mut module =
        parse(&cst).map_err(|diagnostics| diagnostics.with_source_path(source.path()))?;
    for decl in &mut module.structs {
        decl.source_path = source.path().to_path_buf();
    }
    for decl in &mut module.enums {
        decl.source_path = source.path().to_path_buf();
    }
    for function in &mut module.functions {
        function.source_path = source.path().to_path_buf();
    }
    Ok(module)
}

pub fn load_module_graph(source: &SourceFile) -> Result<Module, Diagnostics> {
    Ok(load_module_graph_with_provider(source, &FileSystemSourceProvider)?.module)
}

pub fn load_module_graph_with_provider<Provider>(
    source: &SourceFile,
    provider: &Provider,
) -> Result<LoadedModuleGraph, Diagnostics>
where
    Provider: SourceProvider,
{
    let root_path = provider.normalize_path(source.path());
    let mut resolver = ModuleResolver::new(provider);
    let mut diagnostics = Diagnostics::default();

    resolver.load_source(source.clone(), &mut diagnostics)?;
    let root_imports = resolver
        .modules
        .get(&root_path)
        .map(|module| module.imports.clone())
        .unwrap_or_default();
    let (structs, enums, functions) = resolver.collect_items(&mut diagnostics);

    if diagnostics.is_empty() {
        let sources = resolver
            .load_order
            .iter()
            .filter_map(|path| resolver.sources.get(path).cloned())
            .collect();
        Ok(LoadedModuleGraph {
            module: Module {
                imports: root_imports,
                structs,
                enums,
                functions,
            },
            sources,
        })
    } else {
        Err(diagnostics)
    }
}

impl<'provider, Provider> ModuleResolver<'provider, Provider>
where
    Provider: SourceProvider,
{
    fn new(provider: &'provider Provider) -> Self {
        Self {
            provider,
            modules: HashMap::new(),
            sources: HashMap::new(),
            package_contexts: HashMap::new(),
            load_order: Vec::new(),
            loading_stack: Vec::new(),
        }
    }

    fn load_source(
        &mut self,
        source: SourceFile,
        diagnostics: &mut Diagnostics,
    ) -> Result<(), Diagnostics> {
        let path = self.provider.normalize_path(source.path());
        if self.modules.contains_key(&path) {
            return Ok(());
        }

        let normalized_source = SourceFile::new(path.clone(), source.text().to_string());
        let module = match parse_single_source(&normalized_source) {
            Ok(module) => module,
            Err(source_diagnostics) => {
                diagnostics.0.extend(source_diagnostics.0);
                return Err(std::mem::take(diagnostics));
            }
        };
        self.loading_stack.push(path.clone());

        for import in &module.imports {
            let import_path = match self.resolve_import_path(&path, &import.module) {
                Ok(import_path) => import_path,
                Err(ImportResolutionError::Manifest(error)) => {
                    diagnostics.push(package_manifest_diagnostic(&path, import.span, error));
                    continue;
                }
                Err(ImportResolutionError::ReservedStdlibConflict {
                    stdlib_path,
                    conflicts,
                }) => {
                    diagnostics.push(reserved_stdlib_conflict_diagnostic(
                        &path,
                        &import.module,
                        import.span,
                        &stdlib_path,
                        &conflicts,
                    ));
                    continue;
                }
                Err(ImportResolutionError::Unresolved { searched }) => {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3014",
                            format!("cannot resolve import `{}`", import.module),
                            format!(
                                "searched: {}",
                                searched
                                    .iter()
                                    .map(|candidate| candidate.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            import.span,
                        )
                        .with_fix_it(
                            "create the imported module, add a local dependency in `gof.mod`, or remove the import",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }
            };

            if let Some(position) = self
                .loading_stack
                .iter()
                .position(|candidate| *candidate == import_path)
            {
                let cycle = self.loading_stack[position..]
                    .iter()
                    .chain(std::iter::once(&import_path))
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3015",
                        format!("import cycle detected for `{}`", import.module),
                        format!("the module graph loops back on itself: {cycle}"),
                        import.span,
                    )
                    .with_fix_it(
                        "break the cycle by removing one import edge or extracting shared code",
                    )
                    .with_source_path(path.clone()),
                );
                continue;
            }

            if self.modules.contains_key(&import_path) {
                continue;
            }

            let import_source = match self.provider.load_source(&import_path) {
                Ok(source) => source,
                Err(_) => {
                    diagnostics.push(import_resolution_missing_file_diagnostic(
                        &path,
                        &import.module,
                        import.span,
                        &import_path,
                    ));
                    continue;
                }
            };

            if let Err(import_diagnostics) = self.load_source(import_source, diagnostics) {
                diagnostics.0.extend(import_diagnostics.0);
                return Err(std::mem::take(diagnostics));
            }
        }

        self.loading_stack.pop();
        self.load_order.push(path.clone());
        self.sources.insert(path.clone(), normalized_source);
        self.modules.insert(path, module);
        Ok(())
    }

    fn collect_items(
        &self,
        diagnostics: &mut Diagnostics,
    ) -> (
        Vec<crate::ast::StructDecl>,
        Vec<crate::ast::EnumDecl>,
        Vec<crate::ast::Function>,
    ) {
        let mut seen_structs = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen_enums = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen_methods = HashMap::<(String, String), (PathBuf, crate::source::Span)>::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();

        for path in &self.load_order {
            let module = self
                .modules
                .get(path)
                .expect("loaded module should exist in resolver cache");
            for decl in &module.structs {
                if let Some((original_path, original_span)) = seen_structs.get(&decl.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3020",
                            format!("duplicate struct `{}` in module graph", decl.name),
                            format!(
                                "first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it("rename one of the structs or remove the conflicting import")
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                if let Some((original_path, original_span)) = seen.get(&decl.name).cloned() {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!("top-level name `{}` conflicts between a struct and a function", decl.name),
                            format!(
                                "the function was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it("rename either the struct or the function so constructors stay unambiguous")
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                seen_structs.insert(decl.name.clone(), (path.clone(), decl.span));
                structs.push(decl.clone());
            }

            for decl in &module.enums {
                if let Some((original_path, original_span)) = seen_enums.get(&decl.name).cloned() {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3029",
                            format!("duplicate enum `{}` in module graph", decl.name),
                            format!(
                                "first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it("rename one of the enums or remove the conflicting import")
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                if let Some((original_path, original_span)) = seen_structs.get(&decl.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between an enum and a struct",
                                decl.name
                            ),
                            format!(
                                "the struct was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it(
                            "rename either the enum or the struct so type names stay unambiguous",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                if let Some((original_path, original_span)) = seen.get(&decl.name).cloned() {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between an enum and a function",
                                decl.name
                            ),
                            format!(
                                "the function was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it("rename either the enum or the function so top-level names stay unambiguous")
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                seen_enums.insert(decl.name.clone(), (path.clone(), decl.span));
                enums.push(decl.clone());
            }

            for function in &module.functions {
                if let Some(receiver_type) = &function.receiver_type {
                    let method_key = (receiver_type.name.clone(), function.name.clone());
                    if let Some((original_path, original_span)) =
                        seen_methods.get(&method_key).cloned()
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "GOF3034",
                                format!(
                                    "duplicate method `{}.{}` in module graph",
                                    receiver_type.name, function.name
                                ),
                                format!(
                                    "first declared at {}:{}:{}",
                                    original_path.display(),
                                    original_span.line,
                                    original_span.column
                                ),
                                function.span,
                            )
                            .with_fix_it(
                                "rename one of the methods or remove the conflicting import",
                            )
                            .with_source_path(path.clone()),
                        );
                        continue;
                    }

                    seen_methods.insert(method_key, (path.clone(), function.span));
                    functions.push(function.clone());
                    continue;
                }

                if let Some((original_path, original_span)) =
                    seen_structs.get(&function.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between a function and a struct",
                                function.name
                            ),
                            format!(
                                "the struct was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            function.span,
                        )
                        .with_fix_it(
                            "rename either the function or the struct so calls stay unambiguous",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                if let Some((original_path, original_span)) =
                    seen_enums.get(&function.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between a function and an enum",
                                function.name
                            ),
                            format!(
                                "the enum was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            function.span,
                        )
                        .with_fix_it(
                            "rename either the function or the enum so top-level names stay unambiguous",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                if let Some((original_path, original_span)) = seen.get(&function.name).cloned() {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3016",
                            format!("duplicate function `{}` in module graph", function.name),
                            format!(
                                "first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            function.span,
                        )
                        .with_fix_it("rename one of the functions or remove the conflicting import")
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                seen.insert(function.name.clone(), (path.clone(), function.span));
                functions.push(function.clone());
            }
        }

        (structs, enums, functions)
    }

    fn package_context_for_source(
        &mut self,
        source_path: &Path,
    ) -> Result<Option<PackageContext>, PackageManifestError> {
        let normalized_source = self.provider.normalize_path(source_path);
        if let Some(context) = self.package_contexts.get(&normalized_source) {
            return Ok(context.clone());
        }

        let context = self.provider.package_context_for_source(source_path)?;
        self.package_contexts
            .insert(normalized_source, context.clone());
        Ok(context)
    }

    fn resolve_import_path(
        &mut self,
        current_module_path: &Path,
        module_name: &str,
    ) -> Result<PathBuf, ImportResolutionError> {
        let same_directory_candidate = current_module_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{module_name}.gof"));
        let mut searched = vec![same_directory_candidate.clone()];
        let package_context = self
            .package_context_for_source(current_module_path)
            .map_err(ImportResolutionError::Manifest)?;
        if let Some(stdlib_path) = self.provider.stdlib_module_path(module_name) {
            let mut conflicts = Vec::new();
            if self.provider.is_file(&same_directory_candidate) {
                conflicts.push(format!(
                    "same-directory module at {}",
                    same_directory_candidate.display()
                ));
            }

            if let Some(package_context) = package_context.as_ref() {
                let package_root_candidate = package_context
                    .source_root
                    .join(format!("{module_name}.gof"));
                if package_root_candidate != same_directory_candidate
                    && self.provider.is_file(&package_root_candidate)
                {
                    conflicts.push(format!(
                        "package-root module at {}",
                        package_root_candidate.display()
                    ));
                }

                if let Some(dependency) = package_context.dependencies.get(module_name) {
                    conflicts.push(format!(
                        "local dependency alias `{module_name}` targeting {}",
                        dependency.entry_path.display()
                    ));
                }
            }

            searched.push(stdlib_path.clone());
            if !conflicts.is_empty() {
                return Err(ImportResolutionError::ReservedStdlibConflict {
                    stdlib_path,
                    conflicts,
                });
            }

            if self.provider.is_file(&stdlib_path) {
                return Ok(self.provider.normalize_path(&stdlib_path));
            }
        } else if self.provider.is_file(&same_directory_candidate) {
            return Ok(self.provider.normalize_path(&same_directory_candidate));
        }

        if let Some(package_context) = package_context {
            let package_root_candidate = package_context
                .source_root
                .join(format!("{module_name}.gof"));
            if package_root_candidate != same_directory_candidate {
                searched.push(package_root_candidate.clone());
                if self.provider.is_file(&package_root_candidate) {
                    return Ok(self.provider.normalize_path(&package_root_candidate));
                }
            }

            if let Some(dependency) = package_context.dependencies.get(module_name) {
                searched.push(dependency.entry_path.clone());
                if self.provider.is_file(&dependency.entry_path) {
                    return Ok(self.provider.normalize_path(&dependency.entry_path));
                }
            }
        }

        Err(ImportResolutionError::Unresolved { searched })
    }
}

fn package_manifest_diagnostic(
    source_path: &Path,
    span: crate::source::Span,
    error: PackageManifestError,
) -> Diagnostic {
    match error {
        PackageManifestError::Read { path, message } => Diagnostic::error(
            "GOF3089",
            "invalid package manifest",
            format!("failed to read {}: {message}", path.display()),
            span,
        )
        .with_fix_it("repair `gof.mod` or remove the broken local package configuration")
        .with_source_path(source_path.to_path_buf()),
        PackageManifestError::Parse { path, message } => Diagnostic::error(
            "GOF3089",
            "invalid package manifest",
            format!("failed to parse {}: {message}", path.display()),
            span,
        )
        .with_fix_it("fix the TOML syntax in `gof.mod`")
        .with_source_path(source_path.to_path_buf()),
        PackageManifestError::MissingDependencyManifest {
            manifest,
            dependency,
            expected_manifest,
        } => Diagnostic::error(
            "GOF3089",
            format!("invalid local dependency `{dependency}`"),
            format!(
                "{} declares `{dependency}`, but `{}` does not exist",
                manifest.display(),
                expected_manifest.display()
            ),
            span,
        )
        .with_fix_it("point the dependency at a directory that contains a valid `gof.mod`")
        .with_source_path(source_path.to_path_buf()),
    }
}

fn import_resolution_missing_file_diagnostic(
    source_path: &Path,
    module_name: &str,
    span: crate::source::Span,
    import_path: &Path,
) -> Diagnostic {
    Diagnostic::error(
        "GOF3014",
        format!("cannot resolve import `{module_name}`"),
        format!("expected a module entrypoint at {}", import_path.display()),
        span,
    )
    .with_fix_it(
        "create the imported `.gof` entrypoint, add the missing local dependency file, or remove the import",
    )
    .with_source_path(source_path.to_path_buf())
}

fn reserved_stdlib_conflict_diagnostic(
    source_path: &Path,
    module_name: &str,
    span: crate::source::Span,
    stdlib_path: &Path,
    conflicts: &[String],
) -> Diagnostic {
    Diagnostic::error(
        "GOF3108",
        format!("reserved stdlib import `{module_name}` conflicts with local code"),
        format!(
            "`{module_name}` is reserved for the shipped stdlib entrypoint at {}; conflicting local candidates: {}",
            stdlib_path.display(),
            conflicts.join(", ")
        ),
        span,
    )
    .with_fix_it(
        "rename the local module or dependency alias; shipped stdlib imports `bytes`, `io`, `time`, `net`, and `http` are reserved",
    )
    .with_source_path(source_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::load_module_graph;
    use crate::source::SourceFile;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn loads_functions_from_local_imports() {
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

        let module =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect("module graph should load");

        assert_eq!(module.imports.len(), 1);
        assert_eq!(module.functions.len(), 2);
        assert!(module.enums.is_empty());
        assert_eq!(module.functions[0].name, "square");
        assert_eq!(module.functions[1].name, "main");
    }

    #[test]
    fn rejects_duplicate_function_names_across_modules() {
        let temp = tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("math.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(&helper_path, "fn main() -> int:\n    return 1\n")
            .expect("helper module should be written");
        fs::write(
            &main_path,
            "import math\n\nfn main() -> int:\n    return 2\n",
        )
        .expect("main module should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect_err("duplicate functions should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3016"]);
    }

    #[test]
    fn rejects_import_cycles() {
        let temp = tempdir().expect("tempdir should exist");
        let alpha_path = temp.path().join("alpha.gof");
        let beta_path = temp.path().join("beta.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(
            &alpha_path,
            "import beta\n\nfn alpha() -> int:\n    return beta()\n",
        )
        .expect("alpha module should be written");
        fs::write(
            &beta_path,
            "import alpha\n\nfn beta() -> int:\n    return alpha()\n",
        )
        .expect("beta module should be written");
        fs::write(
            &main_path,
            "import alpha\n\nfn main() -> int:\n    return alpha()\n",
        )
        .expect("main module should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect_err("cycle should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3015"]);
    }

    #[test]
    fn loads_structs_from_local_imports() {
        let temp = tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("geometry.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(
            &helper_path,
            "struct Point:\n    x: int\n    y: int\n\nfn total(point: Point) -> int:\n    return point.x + point.y\n",
        )
        .expect("helper module should be written");
        fs::write(
            &main_path,
            "import geometry\n\nfn main() -> int:\n    point: Point = Point(2, 7)\n    return total(point)\n",
        )
        .expect("main module should be written");

        let module =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect("module graph should load");

        assert_eq!(module.structs.len(), 1);
        assert!(module.enums.is_empty());
        assert_eq!(module.structs[0].name, "Point");
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn loads_enums_from_local_imports() {
        let temp = tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("state.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(&helper_path, "enum Status:\n    Ready\n    Busy\n")
            .expect("helper module should be written");
        fs::write(
            &main_path,
            "import state\n\nfn main() -> bool:\n    return Status.Ready == Status.Busy\n",
        )
        .expect("main module should be written");

        let module =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect("module graph should load");

        assert_eq!(module.enums.len(), 1);
        assert_eq!(module.enums[0].name, "Status");
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn rejects_duplicate_enum_names_across_modules() {
        let temp = tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("state.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(&helper_path, "enum Status:\n    Ready\n")
            .expect("helper module should be written");
        fs::write(
            &main_path,
            "import state\n\nenum Status:\n    Busy\n\nfn main() -> bool:\n    return Status.Ready != Status.Busy\n",
        )
        .expect("main module should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect_err("duplicate enums should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3029"]);
    }

    #[test]
    fn rejects_duplicate_methods_across_modules() {
        let temp = tempdir().expect("tempdir should exist");
        let helper_path = temp.path().join("geometry.gof");
        let main_path = temp.path().join("main.gof");

        fs::write(
            &helper_path,
            "struct Point:\n    x: int\n\nfn Point.total(self: Point) -> int:\n    return self.x\n",
        )
        .expect("helper module should be written");
        fs::write(
            &main_path,
            "import geometry\n\nfn Point.total(self: Point) -> int:\n    return self.x + 1\n\nfn main() -> int:\n    point: Point = Point(3)\n    return point.total()\n",
        )
        .expect("main module should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect_err("duplicate methods should fail");

        assert!(diagnostics.codes().contains(&"GOF3034"));
    }

    #[test]
    fn loads_modules_from_package_source_root() {
        let temp = tempdir().expect("tempdir should exist");
        let manifest_path = temp.path().join("gof.mod");
        let source_root = temp.path().join("src");
        let cmd_root = source_root.join("cmd");
        let helper_path = source_root.join("math.gof");
        let main_path = cmd_root.join("main.gof");

        fs::create_dir_all(&cmd_root).expect("command directory should exist");
        fs::write(
            &manifest_path,
            "module = \"example/app\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("manifest should be written");
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

        let module =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main file should load"))
                .expect("package-root module graph should load");

        assert_eq!(module.functions.len(), 2);
        assert_eq!(module.functions[0].name, "square");
        assert_eq!(module.functions[1].name, "main");
    }

    #[test]
    fn loads_functions_from_local_package_dependencies() {
        let temp = tempdir().expect("tempdir should exist");
        let math_root = temp.path().join("package_math");
        let math_source_root = math_root.join("src");
        let app_root = temp.path().join("package_app");
        let app_source_root = app_root.join("src");
        let math_lib_path = math_source_root.join("lib.gof");
        let math_ops_path = math_source_root.join("ops.gof");
        let app_main_path = app_source_root.join("main.gof");

        fs::create_dir_all(&math_source_root).expect("math source root should exist");
        fs::create_dir_all(&app_source_root).expect("app source root should exist");
        fs::write(
            math_root.join("gof.mod"),
            "module = \"example/package_math\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("math manifest should be written");
        fs::write(
            &math_lib_path,
            "import ops\n\nfn square(value: int) -> int:\n    return multiply(value, value)\n",
        )
        .expect("math lib should be written");
        fs::write(
            &math_ops_path,
            "fn multiply(lhs: int, rhs: int) -> int:\n    return lhs * rhs\n",
        )
        .expect("math ops should be written");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/package_app\"\nedition = \"2026\"\n\n[dependencies]\npackage_math = { path = \"../package_math\" }\n",
        )
        .expect("app manifest should be written");
        fs::write(
            &app_main_path,
            "import package_math\n\nfn main() -> int:\n    return square(9) + square(3)\n",
        )
        .expect("app main should be written");

        let module = load_module_graph(
            &SourceFile::from_path(&app_main_path).expect("app main should load"),
        )
        .expect("local package dependency graph should load");

        assert_eq!(module.functions.len(), 3);
        assert_eq!(module.functions[0].name, "multiply");
        assert_eq!(module.functions[1].name, "square");
        assert_eq!(module.functions[2].name, "main");
    }

    #[test]
    fn rejects_invalid_package_manifests() {
        let temp = tempdir().expect("tempdir should exist");
        let source_root = temp.path().join("src");
        let main_path = source_root.join("main.gof");

        fs::create_dir_all(&source_root).expect("source root should exist");
        fs::write(
            temp.path().join("gof.mod"),
            "module = \"broken\"\nedition = [\n",
        )
        .expect("manifest should be written");
        fs::write(
            &main_path,
            "import math\n\nfn main() -> int:\n    return square(9)\n",
        )
        .expect("main should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main should load"))
                .expect_err("invalid manifest should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3089"]);
    }

    #[test]
    fn loads_reserved_stdlib_modules_from_the_shipped_stdlib_root() {
        let temp = tempdir().expect("tempdir should exist");
        let main_path = temp.path().join("main.gof");

        fs::write(
            &main_path,
            "import http\nimport time\n\nfn main() -> int:\n    return 7\n",
        )
        .expect("main should be written");

        let module =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main should load"))
                .expect("stdlib imports should resolve");

        assert_eq!(module.imports.len(), 2);
        assert!(
            module
                .functions
                .iter()
                .any(|function| function.name == "main"),
            "root module should still expose its own entrypoint",
        );
        assert!(
            module
                .functions
                .iter()
                .any(|function| function.name == "deadline_after"),
            "stdlib `time` imports should contribute their shipped functions",
        );
    }

    #[test]
    fn rejects_same_directory_modules_that_conflict_with_reserved_stdlib_imports() {
        let temp = tempdir().expect("tempdir should exist");
        let main_path = temp.path().join("main.gof");
        let local_http = temp.path().join("http.gof");

        fs::write(&local_http, "fn helper() -> int:\n    return 1\n")
            .expect("local http module should be written");
        fs::write(
            &main_path,
            "import http\n\nfn main() -> int:\n    return 7\n",
        )
        .expect("main should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main should load"))
                .expect_err("reserved stdlib conflicts should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3108"]);
        assert!(
            diagnostics
                .render(&SourceFile::from_path(&main_path).expect("main should reload"))
                .contains("same-directory module"),
            "diagnostic should explain the local conflict",
        );
    }

    #[test]
    fn rejects_local_dependency_aliases_that_conflict_with_reserved_stdlib_imports() {
        let temp = tempdir().expect("tempdir should exist");
        let dep_root = temp.path().join("dep");
        let dep_source_root = dep_root.join("src");
        let app_root = temp.path().join("app");
        let app_source_root = app_root.join("src");
        let main_path = app_source_root.join("main.gof");

        fs::create_dir_all(&dep_source_root).expect("dependency source root should exist");
        fs::create_dir_all(&app_source_root).expect("app source root should exist");
        fs::write(
            dep_root.join("gof.mod"),
            "module = \"example/dep\"\nedition = \"2026\"\n\n[dependencies]\n",
        )
        .expect("dependency manifest should be written");
        fs::write(
            dep_source_root.join("lib.gof"),
            "fn helper() -> int:\n    return 1\n",
        )
        .expect("dependency lib should be written");
        fs::write(
            app_root.join("gof.mod"),
            "module = \"example/app\"\nedition = \"2026\"\n\n[dependencies]\nhttp = { path = \"../dep\" }\n",
        )
        .expect("app manifest should be written");
        fs::write(
            &main_path,
            "import http\n\nfn main() -> int:\n    return 7\n",
        )
        .expect("main should be written");

        let diagnostics =
            load_module_graph(&SourceFile::from_path(&main_path).expect("main should load"))
                .expect_err("reserved stdlib dependency aliases should fail");

        assert_eq!(diagnostics.codes(), vec!["GOF3108"]);
    }
}
