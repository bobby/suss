import {initialize, instantiateRequired} from './bridge.mjs';
import {runJcoProbe} from './jco-probe.mjs';
const result = document.getElementById('result');
function check(condition, message) {
  if (!condition) throw new Error(message);
}
try {
  const features = [];
  for (const feature of ['gc', 'function-references', 'tail-call', 'exceptions']) {
    const response = await fetch(new URL(`./${feature}.wasm`, import.meta.url));
    check(response.ok, `missing feature artifact: ${feature}`);
    const {instance} = await instantiateRequired(await response.arrayBuffer(), {}, feature);
    check(instance.exports.answer() === 42, `wrong executed ${feature} result`);
    features.push(feature);
  }
  // Exercise the diagnostic path with deliberately invalid input. This is an
  // engine compile rejection, not a claim that a valid feature is unsupported.
  let rejected = false;
  try {
    await instantiateRequired(new Uint8Array([0]), {}, 'negative-feature-probe');
  } catch (error) {
    check(error.feature === 'negative-feature-probe', 'missing feature diagnostic');
    check(error.cause instanceof WebAssembly.CompileError, 'original engine diagnostic lost');
    rejected = true;
  }
  check(rejected, 'invalid feature artifact was accepted');
  const runtime = await initialize();
  const first = runtime.addLater(40, 2);
  check(runtime.pending(), 'GC continuation was not rooted before suspension');
  check(await first === 42, 'wrong resumed value');
  check(!runtime.pending(), 'completed frame retained');
  const cancelled = runtime.addLater(100, 1).then(
    () => { throw new Error('cancelled task completed'); },
    error => check(error.message === 'cancelled', 'wrong cancellation error'),
  );
  runtime.cancel();
  check(!runtime.pending(), 'cancelled frame retained');
  // Start immediately, before the cancelled task's queued callback fires.
  const next = runtime.addLater(20, 22);
  await cancelled;
  check(await next === 42, 'stale callback resumed new task');
  check(!runtime.pending(), 'new frame retained');
  const jco = new URL(location.href).searchParams.has('jco') ? await runJcoProbe() : {status: 'not-requested'};
  result.textContent = JSON.stringify({status: 'passed', features, jco, checks: [
    'required core features execute', 'feature compile rejection diagnostic',
    'ES module loading', 'WasmGC continuation', 'Promise suspension and resume',
    'cancellation cleanup', 'stale callback isolation',
  ]});
} catch (error) {
  result.textContent = JSON.stringify({status: 'failed', error: String(error.stack || error)});
}
