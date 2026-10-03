//! Backend object emission + linking (ADR 0004 dev profile, Phase 2
//! slice 3): lowers [`crate::backend_ir::IrModule`] through the SAME
//! shared Cranelift lowering the JIT path uses
//! ([`crate::backend_codegen::compile_module`]) into a REAL object
//! file — machine code on disk — via `cranelift-object`, then links it
//! into a standalone static executable. This is the first static
//! binary produced from the backend IR; no evaluator and no JIT are
//! involved in the artifact.
//!
//! # v1 scope (honest — no silent gaps)
//!
//! The executable contract is EXIT-CODE ONLY: the gof `main`'s int
//! return value becomes the process exit code. Stdio (`print`), string
//! and other heap-value representations, and every construct the JIT
//! slice defers are the SAME named deferrals here (they surface as
//! GOF3180 diagnostics naming the slice, because the lowering is one
//! code path). The linked binary imports exactly one platform
//! library: `kernel32.dll` (the `ExitProcess` call below). There is
//! no C runtime in the artifact.
//!
//! # The entry shim and the exit-code contract
//!
//! A Windows PE entry point's RETURN VALUE DOES NOT become the
//! process exit code — verified empirically across lld-link, MSVC
//! `link.exe`, and MinGW gcc (the entry point IS invoked; a
//! `mov eax,42; ret` entry exits 0). The C runtime never relies on
//! it either: `mainCRTStartup` explicitly calls `exit`. So the object
//! carries one compiler-generated function, the entry shim
//! `__gof_start` (leading double underscores cannot come from gof
//! source identifiers, so it never collides with a user symbol):
//! it calls the gof `main` — which keeps its uniform `() -> i64`
//! v1 value-representation signature, identical to the JIT path, so
//! the two backends share the lowering with zero divergence —
//! truncates the i64 result to 32 bits, and passes it to the
//! imported `ExitProcess`. EXIT CODE = the low 32 bits of the gof
//! int (Windows exit codes are 32-bit unsigned; a gof `-1` exits
//! with `0xFFFFFFFF`). The ABI edge lives in the shim, which is
//! exactly where a calling-convention conversion belongs.
//!
//! # Linker availability (the GOF3181 contract)
//!
//! Linking probes, in order: `lld-link` on `PATH`, `rust-lld` from
//! the active Rust toolchain (driven as the lld-link flavor via
//! `-flavor link`), MSVC `link.exe` located through vswhere (works
//! outside a developer shell), and `gcc` on `PATH` (MinGW). The
//! MSVC-style drivers additionally need the kernel32 import library
//! (an MSVC `LIB` environment or the Windows SDK under
//! `Program Files (x86)\Windows Kits\10\Lib`); gcc ships its own.
//! When no candidate is usable the build reports GOF3181 listing
//! every probed location, and the OBJECT FILE IS STILL WRITTEN next
//! to the output — the machine-code artifact never depends on the
//! linker. A linker that runs and fails reports GOF3182 with its own
//! stderr. v1 static binaries are a Windows contract; on other hosts
//! the probe reports the platform limitation honestly and emission
//! still follows the host triple.
//!
//! # Determinism
//!
//! Emission is deterministic: the same IR emits byte-identical
//! objects (constant file symbol, fixed declaration order, no
//! timestamps) — the same cache-boundary property ADR 0006 gives the
//! serialized IR.
//!
//! Layering (ADR 0002): imports `backend_ir` types +
//! `backend_codegen` (the shared lowering and error type); `pipeline`
//! is the only caller.

use crate::backend_codegen::{CodegenError, compile_module};
use crate::backend_ir::IrModule;
use cranelift::prelude::*;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The static-binary entry symbol (the PE entry point). Leading
/// double underscores are unreachable from gof source identifiers, so
/// the compiler owns this name.
const ENTRY_SYMBOL: &str = "__gof_start";

/// The kernel32 import the entry shim calls: the Windows process-exit
/// API (see the module docs for why a plain entry return is not
/// enough).
const EXIT_PROCESS_SYMBOL: &str = "ExitProcess";

/// Trap after the `ExitProcess` call: `ExitProcess` never returns, so
/// executing this trap means the import resolved to something that
/// did — a broken invariant that fails loudly (a gof-owned user trap
/// code, distinct from the dead-return trap in `backend_codegen`)
/// instead of silently continuing.
const EXIT_RETURNED_TRAP: TrapCode = TrapCode::unwrap_user(2);

/// The file symbol stamped into the emitted object. A constant (the
/// v1 object carries no source-path identity), which keeps emission
/// deterministic.
const FILE_SYMBOL: &str = "gof";

/// The object file extension written beside the linked executable.
const OBJECT_EXTENSION: &str = "obj";

// ---------------------------------------------------------------------------
// Object emission
// ---------------------------------------------------------------------------

/// Compiles the module through `cranelift_object::ObjectModule`: the
/// shared declare/define lowering every machine-code backend runs,
/// plus the static-binary entry shim, emitted as REAL object bytes
/// (COFF on a Windows host, following the host triple elsewhere).
/// Deterministic for identical IR. See the module docs for the v1
/// scope and the exit-code contract.
pub fn emit_object(ir: &IrModule) -> Result<Vec<u8>, CodegenError> {
    let isa = host_isa()?;
    let builder =
        ObjectBuilder::new(isa, FILE_SYMBOL, cranelift_module::default_libcall_names())
            .map_err(|error| CodegenError::BackendError(format!("object setup failed: {error}")))?;
    let mut module = ObjectModule::new(builder);

    // The shared gof lowering (the same passes `execute` runs) — one
    // lowering, two backends.
    let main_id = compile_module(ir, &mut module)?;

    // The only object-path addition to the lowered functions.
    define_entry_shim(&mut module, main_id)?;

    // Backend-specific finalization: the object writer materializes
    // sections, symbols, and relocations into bytes.
    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|error| CodegenError::BackendError(format!("object emission failed: {error}")))?;
    if bytes.is_empty() {
        return Err(CodegenError::BackendError(
            "object emission produced zero bytes".to_string(),
        ));
    }
    Ok(bytes)
}

/// Detects the host ISA the same way `JITBuilder::new` does
/// internally (`cranelift-native`), with default settings and honest
/// errors instead of panics.
fn host_isa() -> Result<cranelift::codegen::isa::OwnedTargetIsa, CodegenError> {
    let isa_builder = cranelift_native::builder().map_err(|reason| {
        CodegenError::BackendError(format!("native target detection failed: {reason}"))
    })?;
    let flags = cranelift::codegen::settings::Flags::new(cranelift::codegen::settings::builder());
    isa_builder.finish(flags).map_err(|error| {
        CodegenError::BackendError(format!("native target construction failed: {error}"))
    })
}

/// Defines the static-binary entry shim `__gof_start` in the module:
/// `call main() -> i64`, truncate to the 32-bit Windows exit code,
/// `call ExitProcess(code)`, then a trap (ExitProcess never returns).
/// The gof `main` keeps its `() -> i64` signature — the shim owns the
/// C-ABI edge — so nothing about the shared lowering changes.
fn define_entry_shim<M: Module>(module: &mut M, main_id: FuncId) -> Result<(), CodegenError> {
    // The imported ExitProcess: one 32-bit argument, no return. The
    // host ISA's default calling convention is the platform ABI
    // (WindowsFastcall on x64 Windows — the single x64 convention),
    // so the call site and the kernel32 export agree.
    let mut exit_signature = module.make_signature();
    exit_signature.params.push(AbiParam::new(types::I32));
    let exit_id = module
        .declare_function(EXIT_PROCESS_SYMBOL, Linkage::Import, &exit_signature)
        .map_err(|error| {
            CodegenError::BackendError(format!("ExitProcess import declaration failed: {error}"))
        })?;

    let shim_signature = module.make_signature();
    let shim_id = module
        .declare_function(ENTRY_SYMBOL, Linkage::Export, &shim_signature)
        .map_err(|error| {
            CodegenError::BackendError(format!("entry shim declaration failed: {error}"))
        })?;

    let mut context = module.make_context();
    // Pre-declare both call references: the builder owns
    // `context.func` exclusively while the shim body is built.
    let main_ref = module.declare_func_in_func(main_id, &mut context.func);
    let exit_ref = module.declare_func_in_func(exit_id, &mut context.func);
    {
        let mut fn_builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut context.func, &mut fn_builder_ctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        let call = builder.ins().call(main_ref, &[]);
        let wide = *builder
            .inst_results(call)
            .first()
            .expect("`main` is declared ()->i64 by `compile_module`");
        let exit_code = builder.ins().ireduce(types::I32, wide);
        builder.ins().call(exit_ref, &[exit_code]);
        builder.ins().trap(EXIT_RETURNED_TRAP);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(shim_id, &mut context)
        .map_err(|error| {
            CodegenError::BackendError(format!("entry shim definition failed: {error}"))
        })?;
    Ok(())
}

/// Where the object file lands for a given executable output path
/// (`out.exe` → the sibling `out.obj`). The object is a first-class
/// v1 artifact — machine code on disk, inspectable with dumpbin — and
/// is kept after linking.
pub fn object_path_for(output_path: &Path) -> PathBuf {
    output_path.with_extension(OBJECT_EXTENSION)
}

// ---------------------------------------------------------------------------
// Linking (the linker-availability contract)
// ---------------------------------------------------------------------------

/// A linker [`link_executable`] can drive, with everything needed to
/// invoke it resolved.
pub(crate) struct ResolvedLinker {
    /// Display name for diagnostics and build output ("lld-link",
    /// "rust-lld", "link.exe", "gcc").
    name: String,
    /// The absolute program path to invoke.
    program: PathBuf,
    /// The argument-vector shape of this driver.
    flavor: LinkerFlavor,
}

/// The argument-vector shape of a resolved linker.
enum LinkerFlavor {
    /// MSVC-style COFF link drivers (`lld-link`, `link.exe`). They
    /// need the kernel32 import library to resolve the ExitProcess
    /// import.
    Msvc { kernel32: PathBuf },
    /// `rust-lld` is the generic LLD driver; the same MSVC-style
    /// options arrive behind `-flavor link`.
    RustLld { kernel32: PathBuf },
    /// The MinGW `gcc` driver: its own library set provides kernel32.
    Gcc,
}

impl ResolvedLinker {
    /// The argument vector linking `object_path` into `output_path`
    /// with the gof entry point.
    fn link_arguments(&self, object_path: &Path, output_path: &Path) -> Vec<std::ffi::OsString> {
        let mut arguments: Vec<std::ffi::OsString> = Vec::new();
        if matches!(self.flavor, LinkerFlavor::RustLld { .. }) {
            arguments.push("-flavor".into());
            arguments.push("link".into());
        }
        match &self.flavor {
            LinkerFlavor::Msvc { kernel32 } | LinkerFlavor::RustLld { kernel32 } => {
                // A standalone no-CRT executable: no default
                // libraries, the gof entry shim as the PE entry point,
                // and exactly one platform import (ExitProcess from
                // kernel32) resolved from its import library.
                arguments.push("/NODEFAULTLIB".into());
                arguments.push("/NOLOGO".into());
                arguments.push(format!("/ENTRY:{ENTRY_SYMBOL}").into());
                arguments.push("/SUBSYSTEM:CONSOLE".into());
                let mut out = std::ffi::OsString::from("/OUT:");
                out.push(output_path);
                arguments.push(out);
                arguments.push(object_path.as_os_str().to_owned());
                arguments.push(kernel32.as_os_str().to_owned());
            }
            LinkerFlavor::Gcc => {
                arguments.push("-nostdlib".into());
                arguments.push("-nostartfiles".into());
                arguments.push("-static".into());
                arguments.push(format!("-Wl,-e,{ENTRY_SYMBOL}").into());
                arguments.push("-o".into());
                arguments.push(output_path.as_os_str().to_owned());
                arguments.push(object_path.as_os_str().to_owned());
                arguments.push("-lkernel32".into());
            }
        }
        arguments
    }
}

/// The outcome of probing the machine for a usable linker: the first
/// usable one, plus one honest line per probed candidate for the
/// GOF3181 diagnostic when none was usable.
pub(crate) struct LinkerProbe {
    pub(crate) linker: Option<ResolvedLinker>,
    pub(crate) probed: Vec<String>,
}

/// Probes the machine for a usable linker, in preference order:
///
/// 1. `lld-link` on `PATH` (an LLVM install)
/// 2. `rust-lld` in the active Rust toolchain (the LLD rustup ships
///    — every Rust developer machine has it)
/// 3. `link.exe` from Visual Studio / Build Tools located through
///    vswhere (works without a developer shell)
/// 4. `gcc` on `PATH` (MinGW)
///
/// The MSVC-style drivers additionally need the kernel32 import
/// library (an MSVC `LIB` environment or the Windows SDK); gcc brings
/// its own. Every candidate contributes one line to the GOF3181
/// diagnostic when it is probed but unusable.
pub(crate) fn probe_linkers() -> LinkerProbe {
    if !cfg!(windows) {
        return LinkerProbe {
            linker: None,
            probed: vec![
                "native object linking is a Windows v1 contract (lld-link / rust-lld / MSVC \
                 link.exe / gcc drivers); a non-Windows entry-point contract is a future slice"
                    .to_string(),
            ],
        };
    }

    let mut probed: Vec<String> = Vec::new();

    if let Some(program) = find_on_path("lld-link") {
        if let Some(linker) = msvc_linker("lld-link", program, &mut probed) {
            return LinkerProbe {
                linker: Some(linker),
                probed,
            };
        }
    } else {
        probed.push("lld-link on PATH: not found".to_string());
    }

    if let Some(program) = find_rust_lld() {
        if let Some(kernel32) = find_kernel32_import_library() {
            return LinkerProbe {
                linker: Some(ResolvedLinker {
                    name: "rust-lld".to_string(),
                    program,
                    flavor: LinkerFlavor::RustLld { kernel32 },
                }),
                probed,
            };
        }
        probed.push(format!(
            "rust-lld found at {} but no kernel32 import library was found for the ExitProcess \
             import (an MSVC `LIB` environment or the Windows SDK under `Program Files \
             (x86)\\Windows Kits\\10\\Lib` is required)",
            program.display()
        ));
    } else {
        probed.push(
            "rust-lld in the active Rust toolchain (`rustc --print sysroot`): not found"
                .to_string(),
        );
    }

    if let Some(program) = find_msvc_link_exe() {
        if let Some(linker) = msvc_linker("link.exe", program, &mut probed) {
            return LinkerProbe {
                linker: Some(linker),
                probed,
            };
        }
    } else {
        probed
            .push("link.exe from Visual Studio with VC tools (via vswhere): not found".to_string());
    }

    if let Some(program) = find_on_path("gcc") {
        return LinkerProbe {
            linker: Some(ResolvedLinker {
                name: "gcc".to_string(),
                program,
                flavor: LinkerFlavor::Gcc,
            }),
            probed,
        };
    }
    probed.push("gcc on PATH: not found".to_string());

    LinkerProbe {
        linker: None,
        probed,
    }
}

/// Wraps an MSVC-style driver candidate: usable only when the
/// kernel32 import library also resolves (the ExitProcess import).
fn msvc_linker(name: &str, program: PathBuf, probed: &mut Vec<String>) -> Option<ResolvedLinker> {
    match find_kernel32_import_library() {
        Some(kernel32) => Some(ResolvedLinker {
            name: name.to_string(),
            program,
            flavor: LinkerFlavor::Msvc { kernel32 },
        }),
        None => {
            probed.push(format!(
                "{name} found but no kernel32 import library was found for the ExitProcess \
                 import (an MSVC `LIB` environment or the Windows SDK under `Program Files \
                 (x86)\\Windows Kits\\10\\Lib` is required)"
            ));
            None
        }
    }
}

/// Writes the object to disk (beside the executable), links it into a
/// standalone executable with the first usable linker on this
/// machine, and verifies the executable was produced. Returns the
/// linker's display name. The object file is written BEFORE the
/// linker probe, so a machine with no usable linker still leaves the
/// machine-code artifact on disk (the GOF3181 contract).
pub fn link_executable(object_bytes: &[u8], output_path: &Path) -> Result<String, CodegenError> {
    link_with_probe(probe_linkers(), object_bytes, output_path)
}

/// The link step with an explicit linker probe — the seam the
/// pipeline tests use for the no-linker (GOF3181) contract;
/// [`link_executable`] always probes the real machine.
pub(crate) fn link_with_probe(
    probe: LinkerProbe,
    object_bytes: &[u8],
    output_path: &Path,
) -> Result<String, CodegenError> {
    let object_path = object_path_for(output_path);
    std::fs::write(&object_path, object_bytes).map_err(|error| CodegenError::LinkFailed {
        detail: format!("failed to write object {}: {error}", object_path.display()),
    })?;

    let linker = probe.linker.ok_or_else(|| CodegenError::NoLinker {
        probed: probe.probed.join("; "),
    })?;

    let invocation = Command::new(&linker.program)
        .args(linker.link_arguments(&object_path, output_path))
        .output();
    let output = match invocation {
        Ok(output) => output,
        Err(error) => {
            return Err(CodegenError::LinkFailed {
                detail: format!("failed to run {}: {error}", linker.program.display()),
            });
        }
    };
    if !output.status.success() {
        return Err(CodegenError::LinkFailed {
            detail: format!(
                "{} exited with {}: {}",
                linker.name,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    if !output_path.is_file() {
        return Err(CodegenError::LinkFailed {
            detail: format!(
                "{} reported success but {} was not produced",
                linker.name,
                output_path.display()
            ),
        });
    }
    Ok(linker.name)
}

// -- linker discovery helpers ----------------------------------------------

/// Finds `name` (or `name.exe`) on `PATH`.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exe = format!("{name}.exe");
    std::env::split_paths(&path)
        .flat_map(move |directory| [directory.join(name), directory.join(&exe)])
        .find(|candidate| candidate.is_file())
}

/// Locates `rust-lld` in the active Rust toolchain: the sysroot
/// printed by `rustc` (found on `PATH`, or through the rustup proxy
/// via `rustup which rustc`) carries it under
/// `lib\rustlib\<host>\bin`.
fn find_rust_lld() -> Option<PathBuf> {
    let rustc = find_on_path("rustc").or_else(rustup_which_rustc)?;
    let sysroot = rustc_print_sysroot(&rustc)?;
    let lld_name = if cfg!(windows) {
        "rust-lld.exe"
    } else {
        "rust-lld"
    };
    let rustlib = sysroot.join("lib").join("rustlib");
    std::fs::read_dir(rustlib)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let candidate = entry.path().join("bin").join(lld_name);
            candidate.is_file().then_some(candidate)
        })
}

/// `rustup which rustc` — resolves the active toolchain's rustc when
/// `PATH` only carries the rustup proxy.
fn rustup_which_rustc() -> Option<PathBuf> {
    let rustup = find_on_path("rustup")?;
    let output = Command::new(rustup)
        .args(["which", "rustc"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let path = PathBuf::from(stdout.lines().next()?.trim());
    path.is_file().then_some(path)
}

/// `rustc --print sysroot` — the active toolchain's sysroot.
fn rustc_print_sysroot(rustc: &Path) -> Option<PathBuf> {
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let path = PathBuf::from(stdout.lines().next()?.trim());
    path.is_dir().then_some(path)
}

/// Locates the kernel32 import library for the ExitProcess import:
/// first the `LIB` search path of an MSVC environment, then the
/// Windows SDK under `Windows Kits\10\Lib\<version>\um\x64` (newest
/// version wins).
fn find_kernel32_import_library() -> Option<PathBuf> {
    if let Some(lib) = std::env::var_os("LIB") {
        if let Some(found) = std::env::split_paths(&lib)
            .map(|directory| directory.join("kernel32.lib"))
            .find(|candidate| candidate.is_file())
        {
            return Some(found);
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(sdk) = std::env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(sdk).join("Lib"));
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(variable) {
            roots.push(
                PathBuf::from(base)
                    .join("Windows Kits")
                    .join("10")
                    .join("Lib"),
            );
        }
    }
    roots.into_iter().find_map(newest_kernel32_in)
}

/// The newest `um\x64\kernel32.lib` under one Windows Kits `Lib` root.
fn newest_kernel32_in(root: PathBuf) -> Option<PathBuf> {
    std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .max_by(|left, right| version_key(left).cmp(&version_key(right)))
        .and_then(|version| {
            let candidate = version.join("um").join("x64").join("kernel32.lib");
            candidate.is_file().then_some(candidate)
        })
}

/// A comparable version key for Windows SDK / MSVC directory names
/// (`10.0.26100.0` → `[10, 0, 26100, 0]`), so version directories
/// order numerically rather than lexicographically.
fn version_key(path: &Path) -> Vec<u64> {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .map(|text| {
            text.split('.')
                .filter_map(|part| part.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Locates the MSVC linker through vswhere: the newest
/// `VC\Tools\MSVC\<version>\bin\Hostx64\x64\link.exe` in the latest
/// Visual Studio (or Build Tools) installation with the VC tools
/// component. This works outside a developer shell — no `LIB` or
/// `PATH` environment required, because the v1 link needs no default
/// libraries.
fn find_msvc_link_exe() -> Option<PathBuf> {
    const VSWHERE: &str = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";
    let output = Command::new(VSWHERE)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let installation = stdout.lines().next()?.trim();
    if installation.is_empty() {
        return None;
    }
    let tools = Path::new(installation)
        .join("VC")
        .join("Tools")
        .join("MSVC");
    std::fs::read_dir(tools)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .max_by(|left, right| version_key(left).cmp(&version_key(right)))
        .map(|version| {
            version
                .join("bin")
                .join("Hostx64")
                .join("x64")
                .join("link.exe")
        })
        .filter(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::{
        CodegenError, LinkerProbe, emit_object, link_executable, link_with_probe, object_path_for,
        probe_linkers,
    };
    use crate::backend_codegen::bind_function_params;
    use crate::backend_ir;
    use crate::hir::lower as lower_hir;
    use crate::mir::lower as lower_mir;
    use crate::module_graph::parse_single_source;
    use crate::source::SourceFile;
    use crate::ssa::{SsaModule, lower as lower_ssa};
    use crate::typed_hir::lower as lower_typed;
    use tempfile::tempdir;

    /// The REAL pipeline chain the build surface uses (the same shape
    /// `backend_codegen`'s tests use).
    fn ssa_from_source(path: &str, source: &str) -> SsaModule {
        let file = SourceFile::new(path, source);
        let ast = parse_single_source(&file).expect("parse should succeed");
        let hir = lower_hir(&ast);
        let (typed, _warnings) = lower_typed(&hir).expect("typing should succeed");
        let mir = lower_mir(&typed);
        lower_ssa(&mir)
    }

    fn object_bytes(path: &str, source: &str) -> Result<Vec<u8>, CodegenError> {
        let ssa = ssa_from_source(path, source);
        let ir = backend_ir::lower(&ssa);
        let param_names: Vec<Vec<String>> = ssa
            .functions
            .iter()
            .map(|function| function.params.clone())
            .collect();
        let bound = bind_function_params(&ir, &param_names)?;
        emit_object(&bound)
    }

    // -- object emission -------------------------------------------------------

    #[test]
    fn object_emission_produces_nonempty_bytes() {
        let bytes = object_bytes("emit.gof", "fn main() -> int:\n    return 2 + 3 * 4\n")
            .expect("object emission should succeed");
        assert!(!bytes.is_empty(), "the object must carry machine code");
    }

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn windows_host_object_is_x64_coff() {
        let bytes = object_bytes("coff.gof", "fn main() -> int:\n    return 1\n")
            .expect("object emission should succeed");
        // A COFF object starts with the machine type: 0x8664 is x64.
        assert_eq!(bytes[0], 0x64);
        assert_eq!(bytes[1], 0x86);
    }

    #[test]
    fn object_emission_is_deterministic() {
        let first = object_bytes("det.gof", "fn main() -> int:\n    return 7\n")
            .expect("first emission should succeed");
        let second = object_bytes("det.gof", "fn main() -> int:\n    return 7\n")
            .expect("second emission should succeed");
        assert_eq!(
            first, second,
            "identical IR must emit byte-identical objects (the cache-boundary property)"
        );
    }

    #[test]
    #[cfg(windows)]
    fn object_carries_the_symbol_contract() {
        // A real structural check: parse the emitted bytes with the
        // `object` reader (the same crate `cranelift-object`
        // re-exports) and verify the symbol surface — the gof
        // functions as defined symbols, the entry shim, and the
        // ExitProcess import as an undefined (external) symbol.
        let bytes = object_bytes(
            "sym.gof",
            "fn helper() -> int:\n    return 3\n\nfn main() -> int:\n    return helper() + 1\n",
        )
        .expect("object emission should succeed");
        use super::{ENTRY_SYMBOL, EXIT_PROCESS_SYMBOL};
        use cranelift_object::object::pe::ImageFileHeader;
        use cranelift_object::object::read::coff::CoffFile;
        use cranelift_object::object::read::{Object as _, ObjectSymbol as _};

        let file: CoffFile<'_, &[u8], ImageFileHeader> =
            CoffFile::parse(&bytes[..]).expect("the emitted bytes must parse as COFF");
        let mut saw_main = false;
        let mut saw_entry = false;
        let mut saw_exit_import = false;
        for symbol in file.symbols() {
            match symbol.name().unwrap_or("") {
                "main" => {
                    assert!(!symbol.is_undefined(), "`main` must be defined");
                    saw_main = true;
                }
                ENTRY_SYMBOL => {
                    assert!(!symbol.is_undefined(), "the entry shim must be defined");
                    saw_entry = true;
                }
                EXIT_PROCESS_SYMBOL => {
                    assert!(symbol.is_undefined(), "`ExitProcess` must be an import");
                    saw_exit_import = true;
                }
                _ => {}
            }
        }
        assert!(saw_main, "the gof `main` must be a compiled symbol");
        assert!(saw_entry, "the `{ENTRY_SYMBOL}` entry shim must be present");
        assert!(saw_exit_import, "the `ExitProcess` import must be present");
    }

    #[test]
    fn match_program_fails_honestly() {
        // The SAME deferral contract as the JIT slice — the lowering
        // is one code path.
        let error = object_bytes(
            "match.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn score(status: Status) -> int:\n    match status:\n        Status.Ready:\n            return 10\n        Status.Busy:\n            return 20\n\nfn main() -> int:\n    status: Status = Status.Busy\n    return score(status)\n",
        )
        .expect_err("match programs must be an honest error");
        match error {
            CodegenError::UnsupportedConstruct { construct } => {
                assert_eq!(construct, "match dispatch");
            }
            other => panic!("expected UnsupportedConstruct, got {other}"),
        }
    }

    // -- linking: THE static-binary proof --------------------------------------

    /// Emits, links, runs, and asserts the process exit code equals
    /// the machine-computed value. On machines with no usable linker
    /// the test skips with a loud note — the no-linker contract is
    /// covered separately by `no_linker_still_lands_the_object`.
    fn run_static_binary(path: &str, source: &str, expected_exit: i32) {
        let bytes = object_bytes(path, source).expect("object emission should succeed");
        let directory = tempdir().expect("temp dir should create");
        let executable = directory.path().join("static.exe");
        match link_executable(&bytes, &executable) {
            Ok(_) => {}
            Err(CodegenError::NoLinker { probed }) => {
                eprintln!("SKIP {path}: no usable linker on this machine: {probed}");
                return;
            }
            Err(other) => panic!("linking should succeed for {path}: {other}"),
        }
        assert!(
            executable.is_file(),
            "the linker must produce the executable"
        );
        let status = std::process::Command::new(&executable)
            .status()
            .expect("the static binary should run");
        assert_eq!(
            status.code(),
            Some(expected_exit),
            "the static binary's exit code must be the machine-computed value for {path}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn static_binary_exits_with_the_gof_return_value() {
        run_static_binary("answer.gof", "fn main() -> int:\n    return 42\n", 42);
    }

    #[test]
    #[cfg(windows)]
    fn static_binary_runs_arithmetic_and_loops() {
        // The loop header re-evaluates its condition per iteration in
        // machine code; sum 0..59 = 1770 proves the loop ran.
        run_static_binary(
            "loop.gof",
            "fn main() -> int:\n    mut total = 0\n    mut index = 0\n    while index < 60:\n        total = total + index\n        index = index + 1\n    return total\n",
            1770,
        );
    }

    #[test]
    #[cfg(windows)]
    fn static_binary_exit_code_truncates_the_i64_value() {
        // 4294967338 = 2^32 + 42: the documented i64 → 32-bit
        // exit-code truncation keeps the low 32 bits.
        run_static_binary("wide.gof", "fn main() -> int:\n    return 4294967338\n", 42);
    }

    #[test]
    fn no_linker_still_lands_the_object_artifact() {
        // The GOF3181 contract through the seam: with no linker, the
        // object bytes still hit disk and the error names every
        // probed location.
        let bytes = object_bytes("nolink.gof", "fn main() -> int:\n    return 1\n")
            .expect("object emission should succeed");
        let directory = tempdir().expect("temp dir should create");
        let executable = directory.path().join("nolink.exe");
        let probe = LinkerProbe {
            linker: None,
            probed: vec!["unit-test: no linker".to_string()],
        };
        match link_with_probe(probe, &bytes, &executable) {
            Err(CodegenError::NoLinker { probed }) => {
                assert!(
                    probed.contains("unit-test: no linker"),
                    "the error lists every probed location: {probed}"
                );
            }
            other => panic!("expected NoLinker, got {other:?}"),
        }
        let object_path = object_path_for(&executable);
        let on_disk =
            std::fs::read(&object_path).expect("the object must still be written without a linker");
        assert_eq!(on_disk, bytes, "the on-disk object is the emitted bytes");
    }

    #[test]
    fn linker_detection_finds_a_linker_or_explains_every_probe() {
        let probe = probe_linkers();
        match probe.linker {
            Some(linker) => {
                // The real machine probe works (this dev machine has
                // rust-lld); the name is reported for build output.
                eprintln!("detected linker: {}", linker.name);
            }
            None => assert!(
                !probe.probed.is_empty(),
                "a failed probe must explain every location it tried"
            ),
        }
    }
}
