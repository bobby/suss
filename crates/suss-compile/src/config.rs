//! Configuration file parser for deps.sus
//!
//! Parses EDN configuration files similar to Clojure's deps.edn.
//!
//! # Example deps.sus
//!
//! ```clojure
//! {:worlds
//!  {:my-app/v1 {:wit "wit/my-app.wit"
//!               :output "target/my-app-v1.wasm"}
//!   :my-app/v2 {:wit "wit/my-app-v2.wit"
//!               :output "target/my-app-v2.wasm"}}
//!  :src-paths ["src"]
//!  :deps
//!  {:wasi/random "0.2.4"
//!   :wasi/cli "0.2.4"}}
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use suss_core::Edn;
use suss_reader::ParserState;

use crate::error::{CompileError, CompileResult};

/// Configuration loaded from deps.sus
#[derive(Debug, Clone)]
pub struct SussConfig {
    /// World definitions (world-name -> world config)
    pub worlds: HashMap<String, WorldConfig>,
    /// Source paths to scan for .sus files
    pub src_paths: Vec<PathBuf>,
    /// Dependencies (package-name -> version)
    pub deps: HashMap<String, String>,
    /// Base directory (where deps.sus is located)
    pub base_dir: PathBuf,
}

/// Configuration for a single world
#[derive(Debug, Clone)]
pub struct WorldConfig {
    /// Path to the WIT file (relative to base_dir)
    pub wit: PathBuf,
    /// Output path for compiled component (relative to base_dir)
    pub output: PathBuf,
}

impl SussConfig {
    /// Load configuration from a deps.sus file
    pub fn load(path: &Path) -> CompileResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

        let base_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        Self::parse(&content, base_dir)
    }

    /// Parse configuration from EDN string
    pub fn parse(content: &str, base_dir: PathBuf) -> CompileResult<Self> {
        let mut parser_state = ParserState::new("suss");
        let edn = suss_reader::parse(content, &mut parser_state)
            .map_err(|e| CompileError::Config(format!("Failed to parse deps.sus: {}", e)))?;

        Self::from_edn(&edn, base_dir)
    }

    /// Convert EDN to config struct
    fn from_edn(edn: &Edn, base_dir: PathBuf) -> CompileResult<Self> {
        let map = match edn {
            Edn::Map(pairs) => pairs,
            _ => return Err(CompileError::Config("deps.sus must be a map".into())),
        };

        let mut worlds = HashMap::new();
        let mut src_paths = vec![base_dir.join("src")]; // Default src path
        let mut deps = HashMap::new();

        for (key, value) in map {
            match key {
                Edn::Keyword(kw) => match kw.name.as_str() {
                    "worlds" => {
                        worlds = Self::parse_worlds(value)?;
                    }
                    "src-paths" => {
                        src_paths = Self::parse_src_paths(value, &base_dir)?;
                    }
                    "deps" => {
                        deps = Self::parse_deps(value)?;
                    }
                    other => {
                        // Ignore unknown keys for forward compatibility
                        eprintln!("Warning: Unknown key in deps.sus: {}", other);
                    }
                },
                _ => {
                    return Err(CompileError::Config(
                        "deps.sus keys must be keywords".into(),
                    ))
                }
            }
        }

        Ok(SussConfig {
            worlds,
            src_paths,
            deps,
            base_dir,
        })
    }

    fn parse_worlds(value: &Edn) -> CompileResult<HashMap<String, WorldConfig>> {
        let map = match value {
            Edn::Map(pairs) => pairs,
            _ => return Err(CompileError::Config(":worlds must be a map".into())),
        };

        let mut worlds = HashMap::new();

        for (key, world_value) in map {
            let world_name = match key {
                Edn::Keyword(kw) => kw.to_string(), // Includes namespace if present
                _ => {
                    return Err(CompileError::Config(
                        "World names must be keywords".into(),
                    ))
                }
            };

            let world_config = Self::parse_world_config(world_value)?;
            worlds.insert(world_name, world_config);
        }

        Ok(worlds)
    }

    fn parse_world_config(value: &Edn) -> CompileResult<WorldConfig> {
        let map = match value {
            Edn::Map(pairs) => pairs,
            _ => return Err(CompileError::Config("World config must be a map".into())),
        };

        let mut wit: Option<PathBuf> = None;
        let mut output: Option<PathBuf> = None;

        for (key, val) in map {
            match key {
                Edn::Keyword(kw) => match kw.name.as_str() {
                    "wit" => {
                        wit = Some(Self::parse_path(val)?);
                    }
                    "output" => {
                        output = Some(Self::parse_path(val)?);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Ok(WorldConfig {
            wit: wit.ok_or_else(|| CompileError::Config("World config missing :wit".into()))?,
            output: output
                .ok_or_else(|| CompileError::Config("World config missing :output".into()))?,
        })
    }

    fn parse_src_paths(value: &Edn, base_dir: &Path) -> CompileResult<Vec<PathBuf>> {
        let vec = match value {
            Edn::Vector(items) => items,
            _ => return Err(CompileError::Config(":src-paths must be a vector".into())),
        };

        vec.iter()
            .map(|item| {
                let path = Self::parse_path(item)?;
                Ok(base_dir.join(path))
            })
            .collect()
    }

    fn parse_deps(value: &Edn) -> CompileResult<HashMap<String, String>> {
        let map = match value {
            Edn::Map(pairs) => pairs,
            _ => return Err(CompileError::Config(":deps must be a map".into())),
        };

        let mut deps = HashMap::new();

        for (key, val) in map {
            let pkg_name = match key {
                Edn::Keyword(kw) => kw.to_string(),
                _ => return Err(CompileError::Config("Dep names must be keywords".into())),
            };

            let version = match val {
                Edn::String(s) => s.clone(),
                _ => return Err(CompileError::Config("Dep versions must be strings".into())),
            };

            deps.insert(pkg_name, version);
        }

        Ok(deps)
    }

    fn parse_path(value: &Edn) -> CompileResult<PathBuf> {
        match value {
            Edn::String(s) => Ok(PathBuf::from(s)),
            _ => Err(CompileError::Config("Path must be a string".into())),
        }
    }

    /// Get absolute path for a world's WIT file
    pub fn wit_path(&self, world_name: &str) -> CompileResult<PathBuf> {
        let world = self
            .worlds
            .get(world_name)
            .ok_or_else(|| CompileError::Config(format!("World '{}' not found", world_name)))?;
        Ok(self.base_dir.join(&world.wit))
    }

    /// Get absolute path for a world's output file
    pub fn output_path(&self, world_name: &str) -> CompileResult<PathBuf> {
        let world = self
            .worlds
            .get(world_name)
            .ok_or_else(|| CompileError::Config(format!("World '{}' not found", world_name)))?;
        Ok(self.base_dir.join(&world.output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_config() {
        let content = r#"
{:worlds
 {:my-app/v1 {:wit "wit/my-app.wit"
              :output "target/my-app.wasm"}}}
"#;
        let config = SussConfig::parse(content, PathBuf::from("/project")).unwrap();

        assert_eq!(config.worlds.len(), 1);
        assert!(config.worlds.contains_key(":my-app/v1"));

        let world = &config.worlds[":my-app/v1"];
        assert_eq!(world.wit, PathBuf::from("wit/my-app.wit"));
        assert_eq!(world.output, PathBuf::from("target/my-app.wasm"));
    }

    #[test]
    fn test_parse_config_with_deps() {
        let content = r#"
{:worlds
 {:example/hello {:wit "hello.wit"
                  :output "hello.wasm"}}
 :deps
 {:wasi/random "0.2.4"
  :wasi/cli "0.2.4"}}
"#;
        let config = SussConfig::parse(content, PathBuf::from(".")).unwrap();

        assert_eq!(config.deps.len(), 2);
        assert_eq!(config.deps.get(":wasi/random"), Some(&"0.2.4".to_string()));
        assert_eq!(config.deps.get(":wasi/cli"), Some(&"0.2.4".to_string()));
    }

    #[test]
    fn test_parse_config_with_src_paths() {
        let content = r#"
{:worlds
 {:app/main {:wit "world.wit"
             :output "app.wasm"}}
 :src-paths ["src" "lib"]}
"#;
        let config = SussConfig::parse(content, PathBuf::from("/project")).unwrap();

        assert_eq!(config.src_paths.len(), 2);
        assert_eq!(config.src_paths[0], PathBuf::from("/project/src"));
        assert_eq!(config.src_paths[1], PathBuf::from("/project/lib"));
    }

    #[test]
    fn test_parse_multiple_worlds() {
        let content = r#"
{:worlds
 {:app/v1 {:wit "wit/v1.wit" :output "target/v1.wasm"}
  :app/v2 {:wit "wit/v2.wit" :output "target/v2.wasm"}}}
"#;
        let config = SussConfig::parse(content, PathBuf::from(".")).unwrap();

        assert_eq!(config.worlds.len(), 2);
        assert!(config.worlds.contains_key(":app/v1"));
        assert!(config.worlds.contains_key(":app/v2"));
    }
}
