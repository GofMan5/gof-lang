//! Incremental backend-IR cache (Phase 2 slice 4; the ADR 0006 cache
//! boundary). The serialized IR is the cache unit: on a HIT the
//! frontend-to-IR pipeline (parse → typed HIR → MIR → SSA → backend
//! IR) is skipped entirely and the decoded module feeds codegen
//! directly — `pipeline::build_module_native_executable_cached` is
//! the caller that proves it.
//!
//! # The cache contract
//!
//! - **Key** (content-addressed): lowercase-hex SHA-256 over a
//!   domain-separated, length-prefixed record: every module-graph
//!   source's (normalized path ‖ bytes) in the graph's deterministic
//!   load order (dependencies before importers; the entry last), then
//!   the IR wire-format version ([`crate::backend_ir::FORMAT_VERSION`]),
//!   then a pipeline salt ([`KEY_SALT`]). The source SET, not just
//!   the entry file, is hashed because one compiled module covers the
//!   whole import closure — a changed import must invalidate the
//!   entry, or a HIT would link a stale program. The salt covers
//!   pipeline changes the wire version does not (lowering behavior);
//!   bumping either is a full cache invalidation by design. The
//!   digest IS the entry file name (with a `.gofir` extension so a
//!   cache directory stays inspectable; the `GOFIR` magic
//!   self-describes the content).
//! - **What is cached**: the IR *after*
//!   `backend_codegen::bind_function_params`. Parameter linkage is
//!   materialized into the module (the binding prepends the
//!   param→local `StoreLocal` instructions and interns the names), so
//!   a decoded entry is the COMPLETE codegen input: no sidecar file,
//!   no second format, and the entry passes the same `decode`
//!   validation the unbound module does. The alternative (caching the
//!   unbound IR plus a param-name side table) would extend the ADR
//!   0006 format for no gain.
//! - **Best-effort, never an error**: the cache is an optimization,
//!   not a dependency. A missing entry, an unreadable file, or a
//!   decode failure (corruption, format drift) is a MISS — the
//!   corrupt file is deleted so the next store replaces it cleanly —
//!   and [`IrCache::store`] silently ignores any write failure. No
//!   cache condition ever fails a build; callers fall back to the
//!   full pipeline.
//! - **Location**: build-local, beside the OUTPUT artifact
//!   (`.gof-ir-cache` directory beside the executable being linked).
//!   `gof build` may point at sources anywhere, so a cache next to
//!   the source would pollute other projects' trees; the output
//!   directory is already where the build writes artifacts (the
//!   `.obj`), and the default output paths live under `target/`. No
//!   user-global state in v1; deleting the directory is always safe.
//! - **Warnings**: a HIT skips the compile pass, so compile-time
//!   warnings are not re-derived (they were advisory and were already
//!   rendered on the MISS build that populated the entry).
//! - **No eviction in v1**: entries are content-addressed, so stale
//!   entries are dead weight, never wrong answers. Garbage collection
//!   (age/size-based) is a later slice; `rm -rf .gof-ir-cache` is the
//!   manual reset.
//!
//! Layering (ADR 0002): imports `backend_ir` (+ `sha2` for the key
//! and `std::fs` for the entries) — nothing else in the crate. The
//! pipeline and CLI are callers; key derivation takes plain
//! `path + bytes` pairs ([`CacheSource`]) so no source-layer type
//! leaks in.

use crate::backend_ir::{FORMAT_VERSION, IrModule, decode, encode};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Key salt: bumps when the key derivation or the cached-artifact
/// SHAPE changes in a way the IR wire version does not see (for
/// example a lowering-behavior change that rewrites emitted IR).
/// Distinct from [`FORMAT_VERSION`], which is itself a key input.
const KEY_SALT: &str = "gof-ir-cache-key-v1";

/// The cache directory name created beside the output artifact.
const CACHE_DIR_NAME: &str = ".gof-ir-cache";

/// The entry file extension (the digest is the file name).
const ENTRY_EXTENSION: &str = "gofir";

/// One hashed key input: a module-graph source's normalized path and
/// bytes. Plain std types on purpose — the pipeline (the caller)
/// extracts these from its `SourceFile`s, so this module imports no
/// source-layer types.
#[derive(Debug, Clone, Copy)]
pub struct CacheSource<'a> {
    pub path: &'a Path,
    pub bytes: &'a [u8],
}

/// Derives the cache key for a module graph: lowercase-hex SHA-256
/// over every source (the graph's deterministic load order:
/// dependencies before importers, the entry last), the IR format
/// version, and the pipeline salt. See the module docs for why the
/// whole set is hashed.
pub fn cache_key(sources: &[CacheSource<'_>]) -> String {
    cache_key_with_salt(sources, KEY_SALT)
}

/// The key derivation with an explicit salt — the seam the
/// version-bump simulation tests use (a future compiler with a new
/// salt must find no old entries). Also the proof that the salt is a
/// real key input.
pub(crate) fn cache_key_with_salt(sources: &[CacheSource<'_>], salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(KEY_DOMAIN.as_bytes());
    write_length(&mut hasher, sources.len() as u64);
    for source in sources {
        let path = source.path.to_string_lossy();
        write_length(&mut hasher, path.len() as u64);
        hasher.update(path.as_bytes());
        write_length(&mut hasher, source.bytes.len() as u64);
        hasher.update(source.bytes);
    }
    hasher.update(&FORMAT_VERSION.to_le_bytes());
    hasher.update(salt.as_bytes());
    let digest = hasher.finalize();
    let mut key = String::with_capacity(digest.len() * 2);
    for byte in digest {
        key.push_str(&format!("{byte:02x}"));
    }
    key
}

/// Domain separator: keys from this derivation never collide with
/// hashes from any other gof use of SHA-256.
const KEY_DOMAIN: &str = "gof-ir-cache-key";

/// Length-prefixes a byte count so concatenated fields cannot be
/// re-associated (path ‖ bytes of one source is unambiguous).
fn write_length(hasher: &mut Sha256, length: u64) {
    hasher.update(&length.to_le_bytes());
}

/// An on-disk IR cache rooted at a directory. Construction is pure:
/// the directory is created lazily by the first [`IrCache::store`],
/// so a cache that is only read never touches the filesystem beyond
/// one failed open.
#[derive(Debug, Clone)]
pub struct IrCache {
    root: PathBuf,
}

impl IrCache {
    /// Opens the cache at an explicit root directory (tests and
    /// future embedders of the compiler).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The default build-local cache for a build output path: the
    /// `.gof-ir-cache` directory beside the artifact being written.
    /// See the module docs for why the cache follows the OUTPUT, not
    /// the source. A bare output name (`app.exe`) has an EMPTY parent
    /// (not `None`), which resolves to the working directory.
    pub fn beside_output(output_path: &Path) -> Self {
        let parent = output_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Self::new(parent.join(CACHE_DIR_NAME))
    }

    /// The cache root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where an entry lives: `<root>/<hex digest>.gofir`.
    pub fn entry_path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.{ENTRY_EXTENSION}"))
    }

    /// Looks a key up. `Some(module)` on a clean hit; `None` on every
    /// miss (missing file, IO error, decode failure). A decode
    /// failure is treated as a corrupt entry: the file is deleted so
    /// the next store replaces it, and the caller sees a plain miss —
    /// the cache never propagates an error.
    pub fn lookup(&self, key: &str) -> Option<IrModule> {
        let path = self.entry_path(key);
        let bytes = std::fs::read(&path).ok()?;
        match decode(&bytes) {
            Ok(module) => Some(module),
            Err(_) => {
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    /// Stores a (param-bound) module under a key. Best-effort: the
    /// write goes to a `.tmp` sibling and is renamed into place, and
    /// ANY failure (directory creation, write, rename) is silently
    /// ignored — the next build just misses. A crash mid-store leaves
    /// either the complete old entry or a `.tmp` orphan (never read),
    /// never a torn entry.
    pub fn store(&self, key: &str, module: &IrModule) {
        let bytes = encode(module);
        if std::fs::create_dir_all(&self.root).is_err() {
            return;
        }
        let path = self.entry_path(key);
        let temp = self.root.join(format!("{key}.{ENTRY_EXTENSION}.tmp"));
        if std::fs::write(&temp, &bytes).is_err() {
            return;
        }
        let _ = std::fs::rename(&temp, &path);
    }

    /// Removes one entry (test seam; a no-op when the entry does not
    /// exist). Best-effort like every other operation.
    pub fn invalidate(&self, key: &str) {
        let _ = std::fs::remove_file(self.entry_path(key));
    }
}

#[cfg(test)]
mod tests {
    use super::{CACHE_DIR_NAME, CacheSource, IrCache, cache_key, cache_key_with_salt};
    use crate::backend_ir::{
        IrBlock, IrFunction, IrInst, IrInstKind, IrModule, IrTerminator, IrTypeTag,
    };
    use std::path::Path;

    /// A minimal VALID module (passes `decode`'s validation): one
    /// `main` function, one param, one constant, one return. Built by
    /// hand so these unit tests import no pipeline layer.
    fn tiny_module() -> IrModule {
        IrModule {
            functions: vec![IrFunction {
                name: 0,
                params: vec![0],
                return_type: IrTypeTag::Int,
                blocks: vec![IrBlock {
                    id: 0,
                    insts: vec![IrInst {
                        result: Some(1),
                        kind: IrInstKind::ConstInt(42),
                    }],
                    terminator: IrTerminator::Return { value: Some(1) },
                }],
            }],
            strings: vec!["main".to_string()],
        }
    }

    fn source<'a>(path: &'a str, bytes: &'a str) -> CacheSource<'a> {
        CacheSource {
            path: Path::new(path),
            bytes: bytes.as_bytes(),
        }
    }

    #[test]
    fn miss_then_hit_round_trips_the_module() {
        let directory = tempfile::tempdir().expect("tempdir should exist");
        let cache = IrCache::new(directory.path());
        let key = cache_key(&[source("main.gof", "fn main() -> int: return 42")]);

        assert!(cache.lookup(&key).is_none(), "a fresh cache must miss");

        let module = tiny_module();
        cache.store(&key, &module);

        let hit = cache.lookup(&key).expect("a stored entry must hit");
        assert_eq!(hit, module, "the entry round-trips exactly");
        assert!(
            cache.entry_path(&key).is_file(),
            "the entry lives under the cache root"
        );
    }

    #[test]
    fn corrupt_entry_is_a_miss_and_self_heals() {
        let directory = tempfile::tempdir().expect("tempdir should exist");
        let cache = IrCache::new(directory.path());
        let key = cache_key(&[source("main.gof", "fn main() -> int: return 42")]);
        cache.store(&key, &tiny_module());

        // Corrupt the stored entry (truncated garbage).
        std::fs::write(cache.entry_path(&key), b"GOFIR-not-a-module").expect("garbage write");

        assert!(
            cache.lookup(&key).is_none(),
            "a corrupt entry must be a MISS, never an error"
        );
        assert!(
            !cache.entry_path(&key).exists(),
            "the corrupt entry is deleted"
        );
    }

    #[test]
    fn store_is_best_effort_when_the_root_cannot_exist() {
        // A root whose parent is a FILE: create_dir_all must fail, so
        // store can only swallow it, and lookup misses.
        let directory = tempfile::tempdir().expect("tempdir should exist");
        let blocker = directory.path().join("blocker");
        std::fs::write(&blocker, b"file").expect("blocker file");
        let cache = IrCache::new(blocker.join(CACHE_DIR_NAME));
        let key = cache_key(&[source("main.gof", "source")]);

        cache.store(&key, &tiny_module());
        assert!(cache.lookup(&key).is_none(), "an unwritable root misses");
    }

    #[test]
    fn invalidate_removes_the_entry() {
        let directory = tempfile::tempdir().expect("tempdir should exist");
        let cache = IrCache::new(directory.path());
        let key = cache_key(&[source("main.gof", "source")]);
        cache.store(&key, &tiny_module());
        cache.invalidate(&key);
        assert!(cache.lookup(&key).is_none(), "invalidated entry misses");
        // Invalidating a missing entry is a no-op, not an error.
        cache.invalidate(&key);
    }

    #[test]
    fn beside_output_places_the_root_next_to_the_artifact() {
        let cache = IrCache::beside_output(Path::new("target/gof/app.exe"));
        assert_eq!(
            cache.root(),
            Path::new("target/gof").join(CACHE_DIR_NAME),
            "the cache follows the output, not the source"
        );
        let cache = IrCache::beside_output(Path::new("app.exe"));
        assert_eq!(
            cache.root(),
            Path::new(".").join(CACHE_DIR_NAME),
            "a bare output name uses the working directory"
        );
    }

    #[test]
    fn key_changes_when_content_changes() {
        let first = cache_key(&[source("main.gof", "return 42")]);
        let second = cache_key(&[source("main.gof", "return 43")]);
        assert_ne!(first, second, "an edited source must change the key");
    }

    #[test]
    fn key_changes_when_a_path_changes() {
        let first = cache_key(&[source("a/main.gof", "same")]);
        let second = cache_key(&[source("b/main.gof", "same")]);
        assert_ne!(first, second, "the source path participates in the key");
    }

    #[test]
    fn key_changes_when_the_import_set_grows() {
        // THE import-correctness property: one compiled module covers
        // the whole graph, so an added import must invalidate the key
        // even though the entry source is byte-identical.
        let entry = source("main.gof", "fn main() -> int: return helper()");
        let helper = source("helper.gof", "fn helper() -> int: return 1");
        let single = cache_key(&[entry]);
        let graph = cache_key(&[entry, helper]);
        assert_ne!(single, graph, "an added import must invalidate the entry");

        // And editing the IMPORT (not the entry) changes the key too.
        let edited_helper = source("helper.gof", "fn helper() -> int: return 2");
        let edited = cache_key(&[entry, edited_helper]);
        assert_ne!(graph, edited, "an edited import must invalidate the entry");
    }

    #[test]
    fn key_changes_when_the_salt_changes() {
        // Version-bump simulation: a future compiler with a new salt
        // finds no entry under the old key.
        let sources = [source("main.gof", "source")];
        let first = cache_key(&sources);
        let bumped = cache_key_with_salt(&sources, "gof-ir-cache-key-v2");
        assert_ne!(first, bumped, "a salt bump is a full invalidation");
        assert_eq!(
            cache_key_with_salt(&sources, "gof-ir-cache-key-v1"),
            first,
            "the default key is the v1 salt"
        );
    }

    #[test]
    fn key_is_a_lowercase_hex_digest() {
        let key = cache_key(&[source("main.gof", "source")]);
        assert_eq!(key.len(), 64, "SHA-256 hex digest length");
        assert!(
            key.chars()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase()),
            "the digest is the file name and must be portable"
        );
    }
}
