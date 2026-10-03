use crate::ast::{Module, parse};
use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::lexer::lex;
use crate::package::{PackageContext, PackageManifestError};
use crate::source::{FileSystemSourceProvider, SourceFile, SourceProvider};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedModuleGraph {
    pub module: Module,
    pub sources: Vec<SourceFile>,
    /// RFC 0053 GOF3172 grandfather-window warnings (blockless imported
    /// modules): warnings never fail the load.
    pub warnings: Diagnostics,
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
    for decl in &mut module.consts {
        decl.source_path = source.path().to_path_buf();
    }
    for protocol in &mut module.protocols {
        protocol.source_path = source.path().to_path_buf();
    }
    for impl_decl in &mut module.impls {
        impl_decl.source_path = source.path().to_path_buf();
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
    let root_exports = resolver
        .modules
        .get(&root_path)
        .map(|module| module.exports.clone())
        .unwrap_or_default();
    // RFC 0054: consts merge across the graph like other top-level
    // declarations (duplicate names report GOF3173 in the typed pass,
    // which owns the const contract).
    let mut root_consts: Vec<crate::ast::ConstDecl> = Vec::new();
    let mut seen_consts: HashMap<String, (PathBuf, crate::source::Span)> = HashMap::new();
    for path in &resolver.load_order {
        let module = resolver
            .modules
            .get(path)
            .expect("loaded module should exist in resolver cache");
        for decl in &module.consts {
            if let Some((original_path, original_span)) = seen_consts.get(&decl.name).cloned() {
                diagnostics.push(
                    Diagnostic::error(
                        "GOF3173",
                        format!("duplicate const `{}` in module graph", decl.name),
                        format!(
                            "first declared at {}:{}:{}",
                            original_path.display(),
                            original_span.line,
                            original_span.column
                        ),
                        decl.span,
                    )
                    .with_fix_it("rename one of the consts or remove the conflicting import")
                    .with_source_path(path.clone()),
                );
                continue;
            }
            seen_consts.insert(decl.name.clone(), (path.clone(), decl.span));
            root_consts.push(decl.clone());
        }
    }
    // RFC 0053: resolve the root's direct imports (path → import span)
    // while the resolver is still mutable, before the shared collection
    // pass.
    let mut direct_imports: HashMap<PathBuf, Option<crate::source::Span>> = HashMap::new();
    for import in &root_imports {
        match resolver.resolve_import_path(&root_path, &import.module) {
            Ok(import_path) => {
                direct_imports.insert(import_path, Some(import.span));
            }
            Err(_) => {
                // Resolution failures already reported during loading.
                direct_imports.insert(PathBuf::new().join(&import.module), None);
            }
        }
    }
    let (structs, enums, functions, protocols, impls) =
        resolver.collect_items(&mut diagnostics, &root_path, &direct_imports);

    if diagnostics.has_errors() {
        return Err(diagnostics);
    }
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
            protocols,
            impls,
            exports: root_exports,
            consts: root_consts,
        },
        sources,
        // RFC 0053 GOF3172 grandfather-window warnings never fail the
        // load; the pipeline threads them to the caller.
        warnings: diagnostics,
    })
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
        root_path: &Path,
        direct_imports: &HashMap<PathBuf, Option<crate::source::Span>>,
    ) -> (
        Vec<crate::ast::StructDecl>,
        Vec<crate::ast::EnumDecl>,
        Vec<crate::ast::Function>,
        Vec<crate::ast::ProtocolDecl>,
        Vec<crate::ast::ImplDecl>,
    ) {
        let mut seen_structs = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen_enums = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen_methods = HashMap::<(String, String), (PathBuf, crate::source::Span)>::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        // RFC 0044: protocol and impl declarations cross the module graph
        // like other top-level names.
        let mut seen_protocols = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut seen_impls = HashMap::<(String, String), (PathBuf, crate::source::Span)>::new();
        let mut protocols = Vec::new();
        let mut impls = Vec::new();

        // RFC 0053 v1 phasing: the merged namespace stays the full
        // graph union (imported modules' bodies typecheck against the
        // same flat table — per-module body views land with per-module
        // compilation in the native backend). The declaration-side
        // contract IS live: `module:` blocks validate (GOF3168),
        // methods require same-file receivers (GOF3170), import
        // collisions of plain names report (GOF3171), and blockless
        // imports warn (GOF3172). Strict per-module filtering and
        // GOF3169 land with the phasing amendment's re-entry criteria.
        let root_path = root_path.to_path_buf();
        let _ = &direct_imports;
        let mut name_origin: HashMap<String, PathBuf> = HashMap::new();

        for path in &self.load_order {
            let module = self
                .modules
                .get(path)
                .expect("loaded module should exist in resolver cache");

            // RFC 0053 v1 phasing: every loaded module merges (imported
            // bodies typecheck against the flat table); the
            // declaration-side contract — export validation (GOF3168),
            // grandfather warnings (GOF3172) — applies per module.
            let is_root = *path == root_path;
            let import_span = direct_imports.get(path).copied().flatten();

            // Per-module export set: a module with a `module:` block
            // publishes exactly the exported names (GOF3168
            // entry-contract validation). v1 keeps the flat merge, so
            // the set is advisory for downstream views but validated
            // here.
            let mut export_set: Option<HashMap<String, ()>> = None;
            if !module.exports.is_empty() {
                let own_names: HashSet<String> = module
                    .structs
                    .iter()
                    .map(|decl| decl.name.clone())
                    .chain(module.enums.iter().map(|decl| decl.name.clone()))
                    .chain(
                        module
                            .functions
                            .iter()
                            .map(|function| function.name.clone()),
                    )
                    .chain(module.protocols.iter().map(|decl| decl.name.clone()))
                    .collect();
                let mut set = HashMap::new();
                for export in &module.exports {
                    if function_is_test_or_fixture_in(module, &export.name)
                        || !own_names.contains(&export.name)
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "GOF3168",
                                format!(
                                    "export entry `{}` does not name a top-level declaration of this module",
                                    export.name
                                ),
                                "entries name this file's own structs, enums, functions, and protocols; test/fixture functions and imported names cannot be exported",
                                export.span,
                            )
                            .with_fix_it(format!(
                                "remove the `{}` entry or declare it in this file",
                                export.name
                            ))
                            .with_source_path(path.clone()),
                        );
                        continue;
                    }
                    set.insert(export.name.clone(), ());
                }
                export_set = Some(set);
            } else if !is_root {
                // Grandfather window: the module has no export block —
                // all its own declarations are currently importable, and
                // the default flips to private at the v1 freeze (GOF3172,
                // warning severity). A module with NO top-level
                // declarations of its own (only methods on builtin types,
                // like the shipped testing module) has no surface to
                // declare and stays silent.
                let has_top_level_surface = !module.structs.is_empty()
                    || !module.enums.is_empty()
                    || !module.protocols.is_empty()
                    || module
                        .functions
                        .iter()
                        .any(|function| function.receiver_type.is_none());
                if import_span.is_some() && has_top_level_surface {
                    diagnostics.push(
                        Diagnostic::warning(
                            "GOF3172",
                            format!(
                                "import `{}` resolves to a module with no `module:` export block",
                                path.file_stem()
                                    .map(|stem| stem.to_string_lossy().to_string())
                                    .unwrap_or_else(|| path.display().to_string())
                            ),
                            "all of the module's own declarations are currently importable; the default flips to private at the v1 freeze, so declare the public surface now",
                            import_span.unwrap_or(crate::source::Span::new(1, 1, 1)),
                        )
                        .with_fix_it(format!(
                            "add a `module:` block to `{}` listing the exported names",
                            path.display()
                        ))
                        .with_source_path(root_path.clone()),
                    );
                }
            }

            let _contributes = |name: &str| -> bool {
                export_set
                    .as_ref()
                    .map(|set| set.contains_key(name))
                    .unwrap_or(true)
            };

            for decl in &module.structs {
                if !is_root {
                    if let Some(other) = name_origin.get(&decl.name) {
                        if other != path {
                            report_import_collision(
                                diagnostics,
                                import_span,
                                path,
                                other,
                                &decl.name,
                                &root_path,
                            );
                            continue;
                        }
                    }
                    name_origin.insert(decl.name.clone(), path.clone());
                }
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
                if !is_root {
                    if let Some(other) = name_origin.get(&decl.name) {
                        if other != path {
                            report_import_collision(
                                diagnostics,
                                import_span,
                                path,
                                other,
                                &decl.name,
                                &root_path,
                            );
                            continue;
                        }
                    }
                    name_origin.insert(decl.name.clone(), path.clone());
                }
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

                // RFC 0044: the protocol↔enum conflict rule is
                // order-independent — this is the reverse arm of the
                // protocol loop's check.
                if let Some((original_path, original_span)) =
                    seen_protocols.get(&decl.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between an enum and a protocol",
                                decl.name
                            ),
                            format!(
                                "the protocol was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it(
                            "rename either the enum or the protocol so type names stay unambiguous",
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
                // RFC 0053 GOF3170: a method declaration requires the
                // receiver struct to live in the SAME file — method sets
                // are compositional, like RFC 0044's impl authority.
                // Builtin opaque types (Bytes, streams, ...) are the
                // shipped-stdlib exception: they are declared by the
                // compiler, and the stdlib modules carry their methods.
                if let Some(receiver_type) = &function.receiver_type {
                    let struct_in_this_file = module
                        .structs
                        .iter()
                        .any(|decl| decl.name == receiver_type.name);
                    let is_builtin_receiver =
                        BUILTIN_RECEIVER_TYPE_NAMES.contains(&receiver_type.name.as_str());
                    if !struct_in_this_file && !is_builtin_receiver {
                        diagnostics.push(
                            Diagnostic::error(
                                "GOF3170",
                                format!(
                                    "method `{}.{}` requires receiver type `{}` to be declared in this module",
                                    receiver_type.name, function.name, receiver_type.name
                                ),
                                "method sets are compositional: a struct's methods live in the struct's own file, mirroring RFC 0044's impl authority rule",
                                function.span,
                            )
                            .with_fix_it(format!(
                                "move the method into the module declaring `{}`, or declare a free function instead",
                                receiver_type.name
                            ))
                            .with_source_path(path.clone()),
                        );
                        continue;
                    }
                }
                // RFC 0053: methods are keyed by (receiver, name), not by
                // their bare name — they never participate in import
                // name collisions and their visibility follows the
                // receiver struct (a method is callable only through a
                // value of the struct, which requires the struct to be
                // visible to construct).
                if function.receiver_type.is_none() && !is_root {
                    if let Some(other) = name_origin.get(&function.name) {
                        if other != path {
                            report_import_collision(
                                diagnostics,
                                import_span,
                                path,
                                other,
                                &function.name,
                                &root_path,
                            );
                            continue;
                        }
                    }
                    name_origin.insert(function.name.clone(), path.clone());
                }
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

            // RFC 0044: protocol declarations join the top-level
            // namespace under the existing conflict rules.
            for decl in &module.protocols {
                if !is_root {
                    if let Some(other) = name_origin.get(&decl.name) {
                        if other != path {
                            report_import_collision(
                                diagnostics,
                                import_span,
                                path,
                                other,
                                &decl.name,
                                &root_path,
                            );
                            continue;
                        }
                    }
                    name_origin.insert(decl.name.clone(), path.clone());
                }
                if let Some((original_path, original_span)) =
                    seen_protocols.get(&decl.name).cloned()
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3132",
                            format!("duplicate protocol `{}` in module graph", decl.name),
                            format!(
                                "first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it("rename one of the protocols or remove the conflicting import")
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
                                "top-level name `{}` conflicts between a protocol and a struct",
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
                            "rename either the protocol or the struct so type names stay unambiguous",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                // RFC 0044: protocol names conflict with enums under the
                // same top-level rule.
                if let Some((original_path, original_span)) = seen_enums.get(&decl.name).cloned() {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3021",
                            format!(
                                "top-level name `{}` conflicts between a protocol and an enum",
                                decl.name
                            ),
                            format!(
                                "the enum was first declared at {}:{}:{}",
                                original_path.display(),
                                original_span.line,
                                original_span.column
                            ),
                            decl.span,
                        )
                        .with_fix_it(
                            "rename either the protocol or the enum so type names stay unambiguous",
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
                                "top-level name `{}` conflicts between a protocol and a function",
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
                        .with_fix_it(
                            "rename either the protocol or the function so top-level names stay unambiguous",
                        )
                        .with_source_path(path.clone()),
                    );
                    continue;
                }

                seen_protocols.insert(decl.name.clone(), (path.clone(), decl.span));
                let mut decl = decl.clone();
                decl.source_path = path.clone();
                protocols.push(decl);
            }

            // RFC 0044: impl declarations — (struct, protocol) pairs are
            // unique across the module graph.
            for decl in &module.impls {
                for protocol in &decl.protocols {
                    let impl_key = (decl.struct_name.clone(), protocol.clone());
                    if let Some((original_path, original_span)) = seen_impls.get(&impl_key).cloned()
                    {
                        diagnostics.push(
                            Diagnostic::error(
                                "GOF3133",
                                format!(
                                    "duplicate conformance `{}: {}` in module graph",
                                    decl.struct_name, protocol
                                ),
                                format!(
                                    "first declared at {}:{}:{}; the struct's own module is the single conformance authority",
                                    original_path.display(),
                                    original_span.line,
                                    original_span.column
                                ),
                                decl.span,
                            )
                            .with_fix_it("remove one of the duplicate impl declarations")
                            .with_source_path(path.clone()),
                        );
                    } else {
                        seen_impls.insert(impl_key, (path.clone(), decl.span));
                    }
                }
                let mut decl = decl.clone();
                decl.source_path = path.clone();
                impls.push(decl);
            }
        }

        (structs, enums, functions, protocols, impls)
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
                        "rename the local module or dependency alias; shipped stdlib imports `bytes`, `io`, `time`, `net`, `http`, and `testing` are reserved",
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

        assert_eq!(diagnostics.codes(), vec!["GOF3172", "GOF3016"]);
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

        assert_eq!(diagnostics.codes(), vec!["GOF3015", "GOF3172"]);
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

        assert_eq!(diagnostics.codes(), vec!["GOF3172", "GOF3029"]);
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

        // RFC 0053: the method's receiver struct lives in the imported
        // module, so GOF3170 (method locality) supersedes the duplicate
        // detection this program used to exercise — the declaration is
        // invalid before any duplicate question arises.
        assert!(diagnostics.codes().contains(&"GOF3170"));
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

        // RFC 0053 v1 phasing: the flat merge still carries transitive
        // names (imported bodies check against the same table);
        // strict per-module filtering lands with per-module views.
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
        assert!(
            module
                .functions
                .iter()
                .any(|function| function.name == "response_status"),
            "stdlib `http` imports should contribute their shipped functions",
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

/// RFC 0053 GOF3170 exemption: builtin opaque types carry methods
/// declared by the shipped stdlib modules (the compiler owns the type;
/// the stdlib files own the method surface). Mirrors the typed layer's
/// builtin-opaque set.
const BUILTIN_RECEIVER_TYPE_NAMES: &[&str] = &[
    "Bytes",
    "ReadStream",
    "WriteStream",
    "DuplexStream",
    "TcpListener",
    "SocketAddr",
    "NetDeadline",
    "TestContext",
    "TempDir",
    "TempFile",
    "FixtureContext",
    "BenchContext",
    "PropertyContext",
];

/// RFC 0053 GOF3171: two direct imports contributing the same name —
/// the error belongs to the importer's choices, reported at the import
/// entry that completes the collision.
/// RFC 0053 GOF3171: two direct imports contributing the same name —
/// the error belongs to the importer's choices, reported at the IMPORT
/// ENTRY that brought the second contribution in, with notes pointing
/// at both source modules.
fn report_import_collision(
    diagnostics: &mut Diagnostics,
    import_span: Option<crate::source::Span>,
    import_source: &Path,
    other_source: &Path,
    name: &str,
    root_path: &Path,
) {
    let span = import_span.unwrap_or(crate::source::Span::new(1, 1, 1));
    diagnostics.push(
        Diagnostic::error(
            "GOF3171",
            format!("two imports both contribute `{name}` into this module"),
            format!(
                "the name arrives from `{}` and `{}`; neither declaration is local, so the ambiguity comes from the import choices",
                other_source.display(),
                import_source.display()
            ),
            span,
        )
        .with_fix_it(format!(
            "rename the `{name}` export in one module, or remove one import"
        ))
        .with_source_path(root_path.to_path_buf()),
    );
}

/// Is the named function a `test fn` or `fixture(scope) fn` in the
/// module? Export entries may not name them (RFC 0053 GOF3168).
fn function_is_test_or_fixture_in(module: &Module, name: &str) -> bool {
    module.functions.iter().any(|function| {
        function.name == name
            && (function.kind == crate::ast::FunctionKind::Test
                || function.kind == crate::ast::FunctionKind::Fixture)
    })
}
