//! Authoritative compiler metadata shared by the owned session and its native
//! callbacks. The Store stays exclusively borrowed by the executing caller.
use super::*;

pub(super) struct CompilerState {
    pub environment: Environment,
    pub cells: Arc<BTreeMap<CellIdentity, Global>>,
    pub linker: Linker<()>,
    pub native_function_cells: Arc<BTreeMap<String, Global>>,
    pub provided: BTreeSet<ModuleIdentity>,
    pub resident: Vec<Instance>,
    pub artifact_bytes: usize,
}

pub(super) type SharedCompiler = Arc<Mutex<CompilerState>>;

/// Never hold the metadata lock while instantiating Wasm or running source.
/// A cheap cell-map snapshot keeps staging independent; drop it before publishing
/// so ordinary mutation need not copy the existing map.
pub(super) fn install(
    shared: &SharedCompiler,
    store: StoreContextMut<'_, ()>,
    engine: &Engine,
    runtime: Instance,
    phase: Phase,
    bytes: &[&[u8]],
    environment: Environment,
    cells: &[CellIdentity],
    interrupt: &AtomicBool,
    evaluation: Option<&Arc<evaluation::Evaluation>>,
) -> Result<Vec<Instance>, SessionError> {
    let (linker, known_cells, native_function_cells) = {
        let state = registry_lock(shared);
        (state.linker.clone(), state.cells.clone(), state.native_function_cells.clone())
    };
    let installed = installation::install(
        store,
        engine,
        runtime,
        phase,
        &linker,
        &known_cells,
        &native_function_cells,
        bytes,
        cells,
        interrupt,
        evaluation,
    )?;
    drop(known_cells);
    let mut state = registry_lock(shared);
    state.linker = installed.linker;
    Arc::make_mut(&mut state.cells).extend(installed.cells);
    state.environment = environment;
    state.resident.extend(installed.instances.iter().copied());
    state.artifact_bytes += installed.artifact_bytes;
    Ok(installed.instances)
}
