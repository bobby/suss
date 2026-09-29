import {initialize} from './bridge.mjs';
const result = document.getElementById('result');
function check(condition, message) {
  if (!condition) throw new Error(message);
}
try {
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
  result.textContent = JSON.stringify({status: 'passed', checks: [
    'ES module loading', 'WasmGC continuation', 'Promise suspension and resume',
    'cancellation cleanup', 'stale callback isolation',
  ]});
} catch (error) {
  result.textContent = JSON.stringify({status: 'failed', error: String(error.stack || error)});
}
