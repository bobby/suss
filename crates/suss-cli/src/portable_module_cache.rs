//! Process-local reuse of compiled code only. Every installation still creates
//! fresh instances and runs the normal initializer/evaluator in its own Store.
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};
use wasmtime::{Engine, Module};

struct Entry {
    bytes: Box<[u8]>,
    module: Module,
}
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
    max_entries: usize,
    max_bytes: usize,
}
impl Cache {
    fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_entries,
            max_bytes,
        }
    }
    fn compile(&mut self, engine: &Engine, bytes: &[u8]) -> wasmtime::Result<Module> {
        // Exact bytes include the emitted ABI/imports/constants. Engine identity
        // additionally fixes the native target, Wasmtime configuration and flags.
        // A digest alone would require collision handling; these bounded entries
        // compare their complete inputs instead.
        if let Some(index) = self.entries.iter().position(|entry| {
            Engine::same(engine, entry.module.engine()) && entry.bytes.as_ref() == bytes
        }) {
            let entry = self.entries.remove(index).unwrap();
            let module = entry.module.clone();
            self.entries.push_back(entry);
            return Ok(module);
        }
        // A failed compilation must not replace a valid cache entry.
        let module = Module::new(engine, bytes)?;
        if self.max_entries == 0 || bytes.len() > self.max_bytes {
            return Ok(module);
        }
        while self.entries.len() >= self.max_entries || self.bytes > self.max_bytes - bytes.len() {
            let removed = self.entries.pop_front().unwrap();
            self.bytes -= removed.bytes.len();
        }
        self.bytes += bytes.len();
        self.entries.push_back(Entry {
            bytes: bytes.into(),
            module: module.clone(),
        });
        Ok(module)
    }
}

pub(crate) fn compile(engine: &Engine, bytes: &[u8]) -> wasmtime::Result<Module> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    // Bounds concern retained input bytes and module handles, not JIT memory or
    // guest heap accounting. The lock also prevents duplicate concurrent builds.
    CACHE
        .get_or_init(|| Mutex::new(Cache::new(64, 32 * 1024 * 1024)))
        .lock()
        .map_err(|_| wasmtime::Error::msg("Compiled module cache lock poisoned"))?
        .compile(engine, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    const EMPTY: &[u8] = b"\0asm\x01\0\0\0";
    fn custom(name: u8) -> Vec<u8> {
        let mut bytes = EMPTY.to_vec();
        bytes.extend([0, 2, 1, name]);
        bytes
    }
    #[test]
    fn production_cache_reuses_the_compiled_module() {
        let engine = Engine::default();
        let first = compile(&engine, EMPTY).unwrap();
        assert!(Module::same(&first, &compile(&engine, EMPTY).unwrap()));
    }
    #[test]
    fn reuse_requires_exact_bytes_and_the_same_engine() {
        let engine = Engine::default();
        let other_engine = Engine::default();
        let mut cache = Cache::new(4, 100);
        let first = cache.compile(&engine, EMPTY).unwrap();
        assert!(Module::same(
            &first,
            &cache.compile(&engine.clone(), EMPTY).unwrap()
        ));
        let other = cache.compile(&other_engine, EMPTY).unwrap();
        assert!(!Module::same(&first, &other));
        assert!(Engine::same(other.engine(), &other_engine));
        let a = cache.compile(&engine, &custom(b'a')).unwrap();
        let b = cache.compile(&engine, &custom(b'b')).unwrap();
        assert!(!Module::same(&a, &b));
        assert!(Module::same(
            &a,
            &cache.compile(&engine, &custom(b'a')).unwrap()
        ));
    }
    #[test]
    fn eviction_is_bounded_and_failures_do_not_displace_code() {
        let engine = Engine::default();
        let mut cache = Cache::new(2, 24);
        let empty = cache.compile(&engine, EMPTY).unwrap();
        cache.compile(&engine, &custom(b'a')).unwrap();
        // Promote the empty module, then evict a.
        assert!(Module::same(
            &empty,
            &cache.compile(&engine, EMPTY).unwrap()
        ));
        cache.compile(&engine, &custom(b'b')).unwrap();
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.bytes, 20);
        assert!(cache.compile(&engine, b"invalid").is_err());
        assert!(Module::same(
            &empty,
            &cache.compile(&engine, EMPTY).unwrap()
        ));
        cache.compile(&engine, &custom(b'a')).unwrap();
        cache.compile(&engine, &custom(b'c')).unwrap();
        assert_eq!(cache.bytes, 24);
        assert!(!Module::same(
            &empty,
            &cache.compile(&engine, EMPTY).unwrap()
        ));
        let mut tiny = Cache::new(1, 8);
        let empty = tiny.compile(&engine, EMPTY).unwrap();
        let oversized = tiny.compile(&engine, &custom(b'a')).unwrap();
        assert!(!Module::same(
            &oversized,
            &tiny.compile(&engine, &custom(b'a')).unwrap()
        ));
        assert!(Module::same(&empty, &tiny.compile(&engine, EMPTY).unwrap()));
        assert_eq!(tiny.bytes, 8);
    }
}
