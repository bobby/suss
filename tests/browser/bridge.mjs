// M0 feasibility bridge only. Production API/ABI is specified in docs/design.
export async function initialize() {
  let active = null;
  let instance;
  const imports = {
    bridge: {
      schedule(delta) {
        const task = active;
        setTimeout(() => {
          // A cancelled task must not resume a subsequent task's GC frame.
          if (active !== task) return;
          try {
            const result = instance.exports.resume(delta);
            active = null;
            task.resolve(result);
          } catch (error) {
            instance.exports.cancel();
            active = null;
            task.reject(error);
          }
        }, 0);
      },
    },
  };
  const bytes = await (await fetch(new URL('./continuation.wasm', import.meta.url))).arrayBuffer();
  ({instance} = await instantiateRequired(bytes, imports, 'GC continuation'));
  return {
    addLater(base, delta) {
      if (active) throw new Error('fixture supports one active task');
      if (![base, delta].every(n => Number.isInteger(n) && n >= -2147483648 && n <= 2147483647)) {
        throw new TypeError('expected signed 32-bit integers');
      }
      return new Promise((resolve, reject) => {
        active = {resolve, reject};
        instance.exports.begin(base, delta);
      });
    },
    cancel() {
      if (!active) return;
      const task = active;
      active = null;
      instance.exports.cancel();
      task.reject(new Error('cancelled'));
    },
    pending: () => instance.exports.pending() !== 0,
  };
}

// Surface the required feature and original engine diagnostic. A compile error
// may be unsupported syntax or bad input; never convert it into a passed probe.
export async function instantiateRequired(bytes, imports, feature) {
  try {
    const module = new WebAssembly.Module(bytes);
    return {module, instance: new WebAssembly.Instance(module, imports)};
  } catch (cause) {
    if (!(cause instanceof WebAssembly.CompileError)) throw cause;
    const error = new Error(`Cannot compile required Wasm feature ${feature}: ${cause.message}`, {cause});
    error.feature = feature;
    throw error;
  }
}
