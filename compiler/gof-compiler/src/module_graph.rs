use crate::ast::{Module, parse};
use crate::cst::CstModule;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::lexer::lex;
use crate::source::SourceFile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct ModuleResolver {
    modules: HashMap<PathBuf, Module>,
    load_order: Vec<PathBuf>,
    loading_stack: Vec<PathBuf>,
}

pub fn parse_single_source(source: &SourceFile) -> Result<Module, Diagnostics> {
    let tokens = lex(source).map_err(|diagnostics| diagnostics.with_source_path(source.path()))?;
    let cst = CstModule::new(tokens);
    let mut module =
        parse(&cst).map_err(|diagnostics| diagnostics.with_source_path(source.path()))?;
    for function in &mut module.functions {
        function.source_path = source.path().to_path_buf();
    }
    Ok(module)
}

pub fn load_module_graph(source: &SourceFile) -> Result<Module, Diagnostics> {
    let root_path = normalize_path(source.path());
    let mut resolver = ModuleResolver::default();
    let mut diagnostics = Diagnostics::default();

    resolver.load_source(source.clone(), &mut diagnostics)?;
    let root_imports = resolver
        .modules
        .get(&root_path)
        .map(|module| module.imports.clone())
        .unwrap_or_default();
    let functions = resolver.collect_functions(&mut diagnostics);

    if diagnostics.is_empty() {
        Ok(Module {
            imports: root_imports,
            functions,
        })
    } else {
        Err(diagnostics)
    }
}

impl ModuleResolver {
    fn load_source(
        &mut self,
        source: SourceFile,
        diagnostics: &mut Diagnostics,
    ) -> Result<(), Diagnostics> {
        let path = normalize_path(source.path());
        if self.modules.contains_key(&path) {
            return Ok(());
        }

        let module = match parse_single_source(&source) {
            Ok(module) => module,
            Err(source_diagnostics) => {
                diagnostics.0.extend(source_diagnostics.0);
                return Err(std::mem::take(diagnostics));
            }
        };
        self.loading_stack.push(path.clone());

        for import in &module.imports {
            let import_path = resolve_import_path(&path, &import.module);

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

            let import_source = match SourceFile::from_path(&import_path) {
                Ok(source) => source,
                Err(_) => {
                    diagnostics.push(
                        Diagnostic::error(
                            "GOF3014",
                            format!("cannot resolve import `{}`", import.module),
                            format!(
                                "expected a sibling module file at {}",
                                import_path.display()
                            ),
                            import.span,
                        )
                        .with_fix_it("create the imported `.gof` file next to the current module or remove the import")
                        .with_source_path(path.clone()),
                    );
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
        self.modules.insert(path, module);
        Ok(())
    }

    fn collect_functions(&self, diagnostics: &mut Diagnostics) -> Vec<crate::ast::Function> {
        let mut seen = HashMap::<String, (PathBuf, crate::source::Span)>::new();
        let mut functions = Vec::new();

        for path in &self.load_order {
            let module = self
                .modules
                .get(path)
                .expect("loaded module should exist in resolver cache");
            for function in &module.functions {
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

        functions
    }
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

fn resolve_import_path(current_module_path: &Path, module_name: &str) -> PathBuf {
    current_module_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{module_name}.gof"))
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
}
