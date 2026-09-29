// Optional packaging feasibility only; no WASI capability or async-value claim.
export async function runJcoProbe() {
  const {instantiate} = await import('./jco/gc-probe.js');
  const api = await instantiate(async path => {
    const response = await fetch(new URL(`./jco/${path}`, import.meta.url));
    if (!response.ok) throw new Error(`missing Jco core artifact: ${path}`);
    return new WebAssembly.Module(await response.arrayBuffer());
  }, {}, (module, imports) => new WebAssembly.Instance(module, imports));
  const value = api.answer();
  if (value !== 42) throw new Error(`wrong Jco GC component result: ${value}`);
  return {status: 'passed', checks: ['transpiled ES module import', 'component export returns 42', 'core GC payload executes']};
}
