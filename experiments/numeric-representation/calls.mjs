// calls.wat's kernels on V8, for the browser target of ADR-0004 (#359).
// Write the module first: target/release/calls --emit target/calls.wasm
// Usage: node calls.mjs <calls.wasm> [n] [reps] [kernel,...]
import { readFileSync } from "node:fs";

const KERNELS = ["inline", "direct", "indirect", "indirect_typed", "table_ref", "proposed", "proposed_subtype",
  "field_typed", "field_untyped", "today"];
const [path, n = "100000000", reps = "5", only = ""] = process.argv.slice(2);
const { instance } = await WebAssembly.instantiate(readFileSync(path));
const { exports } = instance;
exports.setup();
const count = BigInt(n);
for (const name of KERNELS) {
  if (only && !only.split(",").includes(name)) continue;
  let best = Infinity;
  for (let i = 0; i < Number(reps); i++) {
    const start = process.hrtime.bigint();
    const result = exports[name](count);
    const elapsed = Number(process.hrtime.bigint() - start);
    if (result !== count) throw new Error(`${name}: ${result}`);
    best = Math.min(best, elapsed);
  }
  console.log(`${name.padEnd(16)} ${(best / Math.max(Number(n), 1)).toFixed(3).padStart(7)} ns/call`);
}
