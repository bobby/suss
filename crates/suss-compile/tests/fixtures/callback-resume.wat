;; Adapted from Wasmtime v49.0.1 callback-yield-then-exit.wast.
;; SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
;; See wasmtime-fixture-LICENSE and callback-provenance.json.
;; This hand-written feasibility guest is not the production Suss scheduler.
(component
  (import "trace" (func $trace (param "step" u32)))
  (core func $trace (canon lower (func $trace)))
  (core func $task.return (canon task.return (result u32)))
  (core module $guest
    (import "host" "trace" (func $trace (param i32)))
    (import "host" "task.return" (func $task.return (param i32)))
    (global $value (mut i32) (i32.const 0))
    (func (export "enter") (param $value i32) (result i32)
      (global.set $value (local.get $value))
      (call $trace (i32.const 1))
      (i32.const 1)) ;; YIELD: return to the host with the task still pending.
    (func (export "callback") (param i32 i32 i32) (result i32)
      (call $trace (i32.const 2))
      (call $task.return (i32.add (global.get $value) (i32.const 1)))
      (call $trace (i32.const 3))
      (i32.const 0))) ;; EXIT: no continuation remains after task.return.
  (core instance $host
    (export "trace" (func $trace))
    (export "task.return" (func $task.return)))
  (core instance $guest (instantiate $guest (with "host" (instance $host))))
  (func (export "run") async (param "value" u32) (result u32)
    (canon lift (core func $guest "enter") async
      (callback (core func $guest "callback")))))
