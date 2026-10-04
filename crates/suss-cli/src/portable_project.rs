//! Deterministic native project selection over the ordinary compiled AOT pipeline.
use crate::{
    portable_aot::{self, SourceInput},
    portable_repl::read_script_forms,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use suss_compile::{
    SussConfig,
    portable::{
        modules,
        resolve::{self, Phase},
    },
};

fn source(path: PathBuf) -> Result<(SourceInput, Option<String>), String> {
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let forms = read_script_forms(&text).map_err(|error| error.to_string())?;
    let declaration = modules::project_namespace_source(&forms)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let (namespace, world) =
        declaration.map_or((None, None), |(namespace, world)| (Some(namespace), world));
    Ok((
        SourceInput {
            source: text,
            path: Some(path),
            forms,
            namespace,
        },
        world,
    ))
}

fn collect(
    path: &Path,
    directories: &mut BTreeSet<PathBuf>,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let path = std::fs::canonicalize(path)
        .map_err(|error| format!("Failed to resolve source path {}: {error}", path.display()))?;
    if path.is_dir() {
        if !directories.insert(path.clone()) {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(&path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        entries.sort();
        for entry in entries {
            collect(&entry, directories, files)?;
        }
    } else if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("sus" | "cljs" | "cljc")
    ) {
        files.insert(path);
    }
    Ok(())
}

/// Compile all selected worlds before the caller replaces any output artifact.
/// Each world has an isolated compiled Macro session and one staged Runtime
/// catalog. Source targets, explicit entries and dependencies share header rules.
pub fn compile_project(
    config: &SussConfig,
    selected: Option<&str>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if !config.deps.is_empty() {
        return Err("Published project dependency loading remains unimplemented; project :deps cannot be ignored".into());
    }
    let worlds = if let Some(name) = selected {
        let world = config.worlds.get(name).ok_or_else(|| {
            let mut names = config.worlds.keys().cloned().collect::<Vec<_>>();
            names.sort();
            format!("World '{name}' not found in deps.sus. Available: {names:?}")
        })?;
        BTreeMap::from([(name.to_owned(), world)])
    } else {
        config
            .worlds
            .iter()
            .map(|(name, world)| (name.clone(), world))
            .collect()
    };
    let mut discovered = BTreeMap::<String, Vec<SourceInput>>::new();
    if worlds.values().any(|world| world.namespace.is_none()) {
        let mut files = BTreeSet::new();
        let mut directories = BTreeSet::new();
        for root in &config.src_paths {
            collect(root, &mut directories, &mut files)?;
        }
        for path in files {
            let (input, target) = source(path)?;
            if let Some(target) = target {
                discovered.entry(target).or_default().push(input);
            }
        }
    }
    let mut result = BTreeMap::new();
    for (name, world) in worlds {
        let inputs = if let Some(namespace) = &world.namespace {
            let path = resolve::locate_source(namespace, &config.src_paths, 0..0)
                .map_err(|error| error.to_string())?;
            let (input, target) = source(path)?;
            modules::validate_namespace_source(namespace, &input.forms, Phase::Runtime)
                .map_err(|error| error.to_string())?;
            if target.as_ref().is_some_and(|target| target != &name) {
                return Err(format!(
                    "Namespace {namespace} targets {}, not selected project world {name}",
                    target.unwrap()
                ));
            }
            vec![input]
        } else {
            discovered.remove(&name).ok_or_else(|| {
                format!("No source files found with (gen-world {name}) for world '{name}'")
            })?
        };
        for input in &inputs {
            let namespace = input
                .namespace
                .as_deref()
                .expect("selected project namespace");
            let canonical = resolve::locate_source(namespace, &config.src_paths, 0..0)
                .map_err(|error| error.to_string())?;
            if input.path.as_ref() != Some(&canonical) {
                return Err(format!(
                    "Declared namespace {namespace} does not match source path {}",
                    input.path.as_ref().unwrap().display()
                ));
            }
        }
        let bytes = portable_aot::compile_inputs(
            &inputs,
            &config.wit_path(&name).map_err(|error| error.to_string())?,
            world.wit_world.as_deref(),
            &config.src_paths,
            &world.exports,
        )?;
        result.insert(name, bytes);
    }
    Ok(result)
}
