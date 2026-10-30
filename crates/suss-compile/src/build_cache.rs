//! Persistent build directory and validated on-disk compilation caches (#272).
//!
//! Every entry is content-addressed by a SHA-256 key over all of its semantic
//! inputs, carries a digest of its payload, and is published atomically by
//! renaming a completed temporary file. Unreadable, truncated, corrupt,
//! untrusted or incompatible entries are misses: the caller recomputes and
//! republishes. Nothing here holds live Store values, bindings or instances.
//! See `docs/runtime/build-cache.md` for the interface and policies.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

pub mod builds;
pub mod inputs;

/// The on-disk entry format. Changing it changes every entry path, so older
/// entries are never read and are pruned by age.
const FORMAT: &str = "v1";
const MAGIC: &[u8; 8] = b"SUSSBC\0\x01";
const TAG: &str = "Signature: 8a477f597d28d172789f06886806bc55\n\
# This directory holds disposable Suss build caches (`suss cache clean`).\n\
# For information about cache directory tags, see https://bford.info/cachedir/\n";
const DEFAULT_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Temporary files older than this belong to a cancelled or crashed writer.
const STALE_TEMPORARY: Duration = Duration::from_secs(3600);

/// The cache layers, which are also the entry directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Cranelift output for a core module, per engine configuration.
    NativeModule,
    /// Cranelift output for a component, per engine configuration.
    NativeComponent,
    /// Emitted Wasm for one analyzed top-level form or source module.
    Fragment,
    /// The recorded inputs of a completed `suss compile` build.
    BuildManifest,
    /// One output component of a completed build.
    BuildOutput,
}
impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Layer::NativeModule => "native-module",
            Layer::NativeComponent => "native-component",
            Layer::Fragment => "fragment",
            Layer::BuildManifest => "build-manifest",
            Layer::BuildOutput => "build-output",
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}
const LAYERS: usize = 5;

/// Per-process counters, for tests and `SUSS_CACHE_TRACE` evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerStats {
    pub hits: usize,
    pub misses: usize,
    pub stores: usize,
    pub rejects: usize,
}
struct Counters {
    hits: AtomicUsize,
    misses: AtomicUsize,
    stores: AtomicUsize,
    rejects: AtomicUsize,
}
static COUNTERS: [Counters; LAYERS] = [const {
    Counters {
        hits: AtomicUsize::new(0),
        misses: AtomicUsize::new(0),
        stores: AtomicUsize::new(0),
        rejects: AtomicUsize::new(0),
    }
}; LAYERS];
pub fn stats(layer: Layer) -> LayerStats {
    let counters = &COUNTERS[layer.index()];
    LayerStats {
        hits: counters.hits.load(Ordering::Relaxed),
        misses: counters.misses.load(Ordering::Relaxed),
        stores: counters.stores.load(Ordering::Relaxed),
        rejects: counters.rejects.load(Ordering::Relaxed),
    }
}

/// Where the cache lives, if anywhere. Library embedders get no persistent
/// cache unless they configure one or set `SUSS_BUILD_DIR`; the CLI resolves
/// the documented precedence and configures it explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settings {
    Disabled,
    Directory(PathBuf),
}
static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
static RESOLVED: Mutex<Option<Option<&'static BuildDir>>> = Mutex::new(None);

/// Select the process-wide build directory, replacing any earlier choice.
pub fn configure(settings: Settings) {
    *SETTINGS.lock().unwrap_or_else(|error| error.into_inner()) = Some(settings);
    *RESOLVED.lock().unwrap_or_else(|error| error.into_inner()) = None;
}
/// The settings implied by the environment alone: `SUSS_NO_CACHE` disables,
/// otherwise `SUSS_BUILD_DIR` selects a directory.
pub fn environment_settings() -> Option<Settings> {
    if std::env::var_os("SUSS_NO_CACHE").is_some_and(|value| !value.is_empty() && value != "0") {
        return Some(Settings::Disabled);
    }
    std::env::var_os("SUSS_BUILD_DIR")
        .filter(|value| !value.is_empty())
        .map(|value| Settings::Directory(PathBuf::from(value)))
}
/// The build directory this process uses, if caching is enabled.
pub fn current() -> Option<&'static BuildDir> {
    let mut resolved = RESOLVED.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(directory) = *resolved {
        return directory;
    }
    let settings = SETTINGS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .or_else(environment_settings)
        .unwrap_or(Settings::Disabled);
    let directory = match settings {
        Settings::Disabled => None,
        // Settings change rarely (once per process in practice); the leak
        // keeps entry handles 'static without reference counting.
        Settings::Directory(root) => Some(&*Box::leak(Box::new(BuildDir::new(root)))),
    };
    *resolved = Some(directory);
    directory
}

fn tracing() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("SUSS_CACHE_TRACE").is_some_and(|value| value != "0"))
}
pub(crate) fn trace(layer: Layer, event: &str, key: &[u8; 32], detail: &str) {
    let counters = &COUNTERS[layer.index()];
    match event {
        "hit" => &counters.hits,
        "miss" => &counters.misses,
        "store" => &counters.stores,
        _ => &counters.rejects,
    }
    .fetch_add(1, Ordering::Relaxed);
    if tracing() {
        let separator = if detail.is_empty() { "" } else { ": " };
        eprintln!(
            "suss-cache: {} {event} {}{separator}{detail}",
            layer.name(),
            &hex(key)[..16]
        );
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// An incremental key: every field is length-prefixed, so no two field
/// sequences share an encoding.
pub struct Key(Sha256);
impl Key {
    pub fn new(layer: Layer) -> Self {
        let mut key = Key(Sha256::new());
        key.field(b"suss-build-cache");
        key.field(FORMAT.as_bytes());
        key.field(layer.name().as_bytes());
        key
    }
    pub fn field(&mut self, value: &[u8]) -> &mut Self {
        self.0.update((value.len() as u64).to_le_bytes());
        self.0.update(value);
        self
    }
    pub fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

/// A resolved build directory. Layout, all beneath the root:
///   CACHEDIR.TAG                      marks the directory as Suss-owned
///   cache/v1/<layer>/<k0k1>/<key>     validated entries
///   tmp/                              in-progress writes, renamed on success
pub struct BuildDir {
    root: PathBuf,
    owner: OnceLock<Option<u32>>,
    pruned: AtomicBool,
}
impl BuildDir {
    fn new(root: PathBuf) -> Self {
        // Relative settings resolve once, against the starting directory.
        let root = std::path::absolute(&root).unwrap_or(root);
        Self {
            root,
            owner: OnceLock::new(),
            pruned: AtomicBool::new(false),
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn entries(&self) -> PathBuf {
        self.root.join("cache").join(FORMAT)
    }
    fn entry(&self, layer: Layer, key: &[u8; 32]) -> PathBuf {
        let name = hex(key);
        self.entries().join(layer.name()).join(&name[..2]).join(name)
    }
    fn ensure(&self) -> std::io::Result<()> {
        create_dirs(&self.root.join("tmp"))?;
        let tag = self.root.join("CACHEDIR.TAG");
        if !tag.exists() {
            publish_file(&self.root.join("tmp"), &tag, TAG.as_bytes())?;
        }
        Ok(())
    }

    /// The payload of a valid entry, or None after recording why not.
    pub(crate) fn read(&self, layer: Layer, key: &[u8; 32]) -> Option<Vec<u8>> {
        let path = self.entry(layer, key);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                trace(layer, "miss", key, "");
                return None;
            }
            Err(error) => {
                trace(layer, "reject", key, &format!("unreadable: {error}"));
                return None;
            }
        };
        match decode(layer, key, bytes) {
            Ok(payload) => {
                // Recency for pruning; failure only affects eviction order.
                if let Ok(file) = fs::File::options().write(true).open(&path) {
                    let _ = file.set_modified(SystemTime::now());
                }
                Some(payload)
            }
            Err(reason) => {
                trace(layer, "reject", key, reason);
                None
            }
        }
    }
    /// Like `read`, for entries whose payload is native code: additionally
    /// requires that only the current user could have written the entry.
    pub(crate) fn read_trusted(&self, layer: Layer, key: &[u8; 32]) -> Option<Vec<u8>> {
        if let Err(reason) = self.trusted(&self.entry(layer, key)) {
            if self.entry(layer, key).exists() {
                trace(layer, "reject", key, &reason);
            } else {
                trace(layer, "miss", key, "");
            }
            return None;
        }
        self.read(layer, key)
    }
    /// Atomically publish an entry. Failure is never fatal to compilation.
    pub(crate) fn publish(&self, layer: Layer, key: &[u8; 32], payload: &[u8]) {
        let result = (|| {
            self.ensure()?;
            let path = self.entry(layer, key);
            create_dirs(path.parent().unwrap())?;
            publish_file(&self.root.join("tmp"), &path, &encode(layer, key, payload))
        })();
        match result {
            Ok(()) => trace(layer, "store", key, ""),
            Err(error) => trace(layer, "reject", key, &format!("not stored: {error}")),
        }
        if !self.pruned.swap(true, Ordering::Relaxed) {
            let _ = self.prune(max_bytes());
        }
    }

    /// The trust policy for loading native code (see the module docs): the
    /// entry and every directory from it up to the build root must belong to
    /// the current user and be writable by no one else.
    #[cfg(unix)]
    fn trusted(&self, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::MetadataExt;
        let Some(owner) = self.owner() else {
            return Err("cannot determine the current user".into());
        };
        let mut current = Some(path);
        while let Some(path) = current {
            let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err(format!("{} is a symbolic link", path.display()));
            }
            if metadata.uid() != owner {
                return Err(format!("{} belongs to another user", path.display()));
            }
            if metadata.mode() & 0o022 != 0 {
                return Err(format!("{} is writable by other users", path.display()));
            }
            if path == self.root {
                return Ok(());
            }
            current = path.parent();
        }
        Err("entry is outside the build directory".into())
    }
    #[cfg(not(unix))]
    fn trusted(&self, path: &Path) -> Result<(), String> {
        // Without POSIX ownership, the build directory's own access control
        // is the boundary; see docs/runtime/build-cache.md.
        if path.exists() { Ok(()) } else { Err("missing".into()) }
    }
    /// The effective user, read from a file this process creates: no libc.
    #[cfg(unix)]
    fn owner(&self) -> Option<u32> {
        use std::os::unix::fs::MetadataExt;
        *self.owner.get_or_init(|| {
            self.ensure().ok()?;
            let probe = tempfile_in(&self.root.join("tmp"), 0o600).ok()?;
            let uid = fs::metadata(&probe.0).ok()?.uid();
            drop(probe);
            Some(uid)
        })
    }

    /// Remove cached state only, after confirming the directory is ours.
    /// Source and final outputs never live beneath the build directory.
    pub fn clean(&self) -> Result<u64, String> {
        if !self.root.exists() {
            return Ok(0);
        }
        let tag = fs::read_to_string(self.root.join("CACHEDIR.TAG")).unwrap_or_default();
        if !tag.starts_with(TAG.lines().next().unwrap()) || !tag.contains("Suss build caches") {
            return Err(format!(
                "{} is not a Suss build directory (no Suss CACHEDIR.TAG); nothing removed",
                self.root.display()
            ));
        }
        let mut removed = 0;
        for name in ["cache", "tmp"] {
            let path = self.root.join(name);
            removed += size(&path);
            match fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("removing {}: {error}", path.display())),
            }
        }
        Ok(removed)
    }
    /// Total bytes of cached entries and temporaries.
    pub fn size(&self) -> u64 {
        size(&self.root.join("cache")) + size(&self.root.join("tmp"))
    }

    /// Evict least recently used entries until the cache is under 3/4 of
    /// `limit`, and remove abandoned temporaries. Concurrent pruners may
    /// remove an entry another process is about to read; that is a miss.
    pub fn prune(&self, limit: u64) -> std::io::Result<()> {
        let mut entries = Vec::new();
        let mut total = 0;
        collect(&self.root.join("cache"), &mut entries, &mut total)?;
        if let Ok(temporaries) = fs::read_dir(self.root.join("tmp")) {
            for temporary in temporaries.flatten() {
                if temporary
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age > STALE_TEMPORARY)
                {
                    let _ = fs::remove_file(temporary.path());
                }
            }
        }
        if total <= limit {
            return Ok(());
        }
        entries.sort_by_key(|(modified, _, _)| *modified);
        for (_, length, path) in entries {
            if total <= limit / 4 * 3 {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= length;
            }
        }
        Ok(())
    }
}

fn max_bytes() -> u64 {
    std::env::var("SUSS_CACHE_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_MAX_BYTES)
}
fn size(path: &Path) -> u64 {
    let mut entries = Vec::new();
    let mut total = 0;
    let _ = collect(path, &mut entries, &mut total);
    total
}
fn collect(
    path: &Path,
    entries: &mut Vec<(SystemTime, u64, PathBuf)>,
    total: &mut u64,
) -> std::io::Result<()> {
    let listing = match fs::read_dir(path) {
        Ok(listing) => listing,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in listing.flatten() {
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect(&entry.path(), entries, total)?;
        } else {
            *total += metadata.len();
            entries.push((
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                metadata.len(),
                entry.path(),
            ));
        }
    }
    Ok(())
}

/// Header: magic, layer name, key, payload length, payload SHA-256.
fn encode(layer: Layer, key: &[u8; 32], payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len() + 128);
    bytes.extend(MAGIC);
    bytes.push(layer.name().len() as u8);
    bytes.extend(layer.name().as_bytes());
    bytes.extend(key);
    bytes.extend((payload.len() as u64).to_le_bytes());
    bytes.extend(Sha256::digest(payload));
    bytes.extend(payload);
    bytes
}
fn decode(layer: Layer, key: &[u8; 32], mut bytes: Vec<u8>) -> Result<Vec<u8>, &'static str> {
    let name = layer.name().as_bytes();
    let header = MAGIC.len() + 1 + name.len() + 32 + 8 + 32;
    if bytes.len() < header {
        return Err("truncated header");
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err("unknown entry format");
    }
    let mut at = MAGIC.len();
    if bytes[at] as usize != name.len() || &bytes[at + 1..at + 1 + name.len()] != name {
        return Err("entry belongs to another layer");
    }
    at += 1 + name.len();
    if &bytes[at..at + 32] != key {
        return Err("entry key mismatch");
    }
    at += 32;
    let length = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    at += 8;
    if bytes.len() as u64 - header as u64 != length {
        return Err("truncated or extended payload");
    }
    let digest: [u8; 32] = bytes[at..at + 32].try_into().unwrap();
    if Sha256::digest(&bytes[header..])[..] != digest {
        return Err("payload digest mismatch");
    }
    bytes.drain(..header);
    Ok(bytes)
}

/// Create directories with explicit owner-only write permission, so a
/// permissive umask (such as a user-private-group 002) cannot make the cache
/// fail the native-code trust policy.
fn create_dirs(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o755);
    builder.create(path)
}
/// A uniquely named temporary file, removed on drop unless persisted.
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn tempfile_in(directory: &Path, mode: u32) -> std::io::Result<Temporary> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    loop {
        let name = format!(
            ".tmp-{}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|elapsed| elapsed.subsec_nanos())
                .unwrap_or(0)
        );
        let path = directory.join(name);
        let mut options = fs::File::options();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
        #[cfg(not(unix))]
        let _ = mode;
        match options.open(&path) {
            Ok(_) => return Ok(Temporary(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}
/// Write `bytes` beside nothing visible, then rename over `target`: readers
/// see either the previous complete file or the new complete file.
fn publish_file(temporaries: &Path, target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    publish_with_mode(temporaries, target, bytes, 0o644)
}
/// Cache entries are 0644 regardless of umask; outputs keep umask defaults.
fn publish_with_mode(temporaries: &Path, target: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let temporary = tempfile_in(temporaries, mode)?;
    {
        let mut file = fs::File::options().write(true).truncate(true).open(&temporary.0)?;
        file.write_all(bytes)?;
        file.sync_data()?;
    }
    fs::rename(&temporary.0, target)?;
    std::mem::forget(temporary);
    Ok(())
}

/// Atomically replace a final output: a temporary in the output's own
/// directory is renamed over it, so a failed or interrupted write leaves the
/// previous output intact and never exposes a partial file.
pub fn write_output(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    publish_with_mode(&directory, path, bytes, 0o666)
}

#[cfg(not(target_family = "wasm"))]
mod native {
    use super::*;
    use std::hash::{Hash, Hasher};
    use wasmtime::{Engine, Module, component::Component};

    fn key(layer: Layer, engine: &Engine, bytes: &[u8]) -> [u8; 32] {
        // The compatibility hash covers the Wasmtime version, target and the
        // configuration that affects generated code; deserialization checks
        // it again, so a disagreement is an incompatible-entry miss.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        engine.precompile_compatibility_hash().hash(&mut hasher);
        let mut key = Key::new(layer);
        key.field(&hasher.finish().to_le_bytes())
            .field(Sha256::digest(bytes).as_slice());
        key.finish()
    }
    /// Compile a core module, reusing validated native code when trusted.
    pub(crate) fn module(engine: &Engine, bytes: &[u8]) -> wasmtime::Result<Module> {
        let Some(directory) = current() else {
            return Module::new(engine, bytes);
        };
        let key = key(Layer::NativeModule, engine, bytes);
        if let Some(payload) = directory.read_trusted(Layer::NativeModule, &key) {
            // SAFETY: the trust policy admits only entries that this user
            // alone could have written, published by `serialize` below for an
            // engine with this compatibility hash; their integrity digest has
            // been checked, and Wasmtime rejects incompatible headers.
            match unsafe { Module::deserialize(engine, &payload) } {
                Ok(module) => {
                    trace(Layer::NativeModule, "hit", &key, "");
                    return Ok(module);
                }
                Err(error) => trace(Layer::NativeModule, "reject", &key, &error.to_string()),
            }
        }
        let module = Module::new(engine, bytes)?;
        if let Ok(serialized) = module.serialize() {
            directory.publish(Layer::NativeModule, &key, &serialized);
        }
        Ok(module)
    }
    /// Compile a component, reusing validated native code when trusted.
    pub fn component(engine: &Engine, bytes: &[u8]) -> wasmtime::Result<Component> {
        let Some(directory) = current() else {
            return Component::new(engine, bytes);
        };
        let key = key(Layer::NativeComponent, engine, bytes);
        if let Some(payload) = directory.read_trusted(Layer::NativeComponent, &key) {
            // SAFETY: as for `module`.
            match unsafe { Component::deserialize(engine, &payload) } {
                Ok(component) => {
                    trace(Layer::NativeComponent, "hit", &key, "");
                    return Ok(component);
                }
                Err(error) => trace(Layer::NativeComponent, "reject", &key, &error.to_string()),
            }
        }
        let component = Component::new(engine, bytes)?;
        if let Ok(serialized) = component.serialize() {
            directory.publish(Layer::NativeComponent, &key, &serialized);
        }
        Ok(component)
    }
}
#[cfg(not(target_family = "wasm"))]
pub use native::component;
#[cfg(not(target_family = "wasm"))]
pub(crate) use native::module;

/// The directory a CLI process uses, by documented precedence: an explicit
/// flag, then `SUSS_NO_CACHE`/`SUSS_BUILD_DIR`, then a project's
/// `:build-dir`, then `target/suss` beside the nearest `deps.sus` at or above
/// `start`, then the user cache directory.
pub fn cli_settings(
    flag: Option<Settings>,
    project_build_dir: Option<PathBuf>,
    start: &Path,
) -> Settings {
    if let Some(flag) = flag {
        return flag;
    }
    if let Some(environment) = environment_settings() {
        return environment;
    }
    if let Some(directory) = project_build_dir {
        return Settings::Directory(directory);
    }
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_path_buf());
    for directory in start.ancestors() {
        if directory.join("deps.sus").is_file() {
            return Settings::Directory(directory.join("target").join("suss"));
        }
    }
    user_cache_dir()
        .map(|directory| Settings::Directory(directory.join("suss")))
        .unwrap_or(Settings::Disabled)
}
fn user_cache_dir() -> Option<PathBuf> {
    let absolute = |value: std::ffi::OsString| Some(PathBuf::from(value)).filter(|path| path.is_absolute());
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").and_then(absolute);
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .and_then(absolute)
            .map(|home| home.join("Library").join("Caches"));
    }
    std::env::var_os("XDG_CACHE_HOME")
        .and_then(absolute)
        .or_else(|| std::env::var_os("HOME").and_then(absolute).map(|home| home.join(".cache")))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> (tempfile::TempDir, BuildDir) {
        let root = tempfile::tempdir().unwrap();
        let directory = BuildDir::new(root.path().join("build"));
        (root, directory)
    }
    #[test]
    fn entries_round_trip_and_every_corruption_is_a_miss() {
        let (_root, directory) = directory();
        let key = Key::new(Layer::Fragment).finish();
        assert_eq!(directory.read(Layer::Fragment, &key), None);
        directory.publish(Layer::Fragment, &key, b"payload");
        assert_eq!(directory.read(Layer::Fragment, &key).unwrap(), b"payload");
        // Another layer never reads it, even with the same key.
        assert_eq!(directory.read(Layer::BuildOutput, &key), None);
        let path = directory.entry(Layer::Fragment, &key);
        let valid = fs::read(&path).unwrap();
        let mut corruptions = vec![
            valid[..valid.len() - 1].to_vec(),
            valid[..10].to_vec(),
            Vec::new(),
            [valid.clone(), vec![0]].concat(),
        ];
        for index in [0, 9, 20, valid.len() - 1] {
            let mut flipped = valid.clone();
            flipped[index] ^= 1;
            corruptions.push(flipped);
        }
        for corrupt in corruptions {
            fs::write(&path, &corrupt).unwrap();
            assert_eq!(directory.read(Layer::Fragment, &key), None);
        }
        directory.publish(Layer::Fragment, &key, b"payload");
        assert_eq!(directory.read(Layer::Fragment, &key).unwrap(), b"payload");
    }
    #[cfg(unix)]
    #[test]
    fn native_entries_require_an_owner_only_writable_path() {
        use std::os::unix::fs::PermissionsExt;
        let (_root, directory) = directory();
        let key = Key::new(Layer::NativeModule).finish();
        directory.publish(Layer::NativeModule, &key, b"code");
        let path = directory.entry(Layer::NativeModule, &key);
        // Directories created under a permissive umask are rejected too.
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode();
        for ancestor in path.ancestors().take_while(|path| path.starts_with(directory.root())) {
            fs::set_permissions(ancestor, fs::Permissions::from_mode(mode(ancestor) & !0o022)).unwrap();
        }
        assert_eq!(directory.read_trusted(Layer::NativeModule, &key).unwrap(), b"code");
        let parent = path.parent().unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(mode(parent) | 0o020)).unwrap();
        assert_eq!(directory.read_trusted(Layer::NativeModule, &key), None);
        fs::set_permissions(parent, fs::Permissions::from_mode(mode(parent) & !0o020)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(directory.read_trusted(Layer::NativeModule, &key), None);
    }
    #[test]
    fn clean_removes_only_cache_state_of_a_tagged_directory() {
        let (root, directory) = directory();
        fs::create_dir_all(directory.root()).unwrap();
        fs::write(directory.root().join("keep.sus"), "(ns keep)").unwrap();
        assert!(directory.clean().is_err(), "untagged directories are refused");
        assert!(directory.root().join("keep.sus").exists());
        let key = Key::new(Layer::Fragment).finish();
        directory.publish(Layer::Fragment, &key, b"payload");
        assert!(directory.size() > 0);
        assert!(directory.clean().unwrap() > 0);
        assert_eq!(directory.size(), 0);
        assert!(directory.root().join("keep.sus").exists());
        assert!(directory.root().join("CACHEDIR.TAG").exists());
        assert!(root.path().exists());
    }
    #[test]
    fn prune_evicts_least_recently_used_entries_and_stale_temporaries() {
        let (_root, directory) = directory();
        let keys: Vec<_> = (0..4u8)
            .map(|index| {
                let mut key = Key::new(Layer::Fragment);
                key.field(&[index]);
                key.finish()
            })
            .collect();
        for (index, key) in keys.iter().enumerate() {
            directory.publish(Layer::Fragment, key, &[0; 1000]);
            let file = fs::File::options()
                .write(true)
                .open(directory.entry(Layer::Fragment, key))
                .unwrap();
            file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + index as u64))
                .unwrap();
        }
        let stale = directory.root().join("tmp").join(".tmp-stale");
        fs::write(&stale, b"partial").unwrap();
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
        let entry = fs::metadata(directory.entry(Layer::Fragment, &keys[0])).unwrap().len();
        directory.prune(entry * 2).unwrap();
        assert!(!stale.exists());
        let kept: Vec<_> = keys
            .iter()
            .map(|key| directory.entry(Layer::Fragment, key).exists())
            .collect();
        assert_eq!(kept, [false, false, false, true]);
    }
    #[test]
    fn cli_precedence_prefers_flags_then_project_then_nearest_deps() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("app/src/deep");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.path().join("app/deps.sus"), "{}").unwrap();
        // The environment may select a directory for this test process.
        if environment_settings().is_none() {
            assert_eq!(
                cli_settings(None, None, &nested),
                Settings::Directory(root.path().join("app/target/suss"))
            );
            assert_eq!(
                cli_settings(None, Some(PathBuf::from("/x")), &nested),
                Settings::Directory(PathBuf::from("/x"))
            );
        }
        assert_eq!(
            cli_settings(Some(Settings::Disabled), Some(PathBuf::from("/x")), &nested),
            Settings::Disabled
        );
    }
}
