;; Original M0-02 async lower / cancellation / callback feasibility fixture.
;; This is not Suss-generated scheduling code.
(component
  (import "increment" (func $increment async (param "value" u32) (result u32)))
  (import "trace" (func $trace (param "step" u32)))
  (core module $memory (memory (export "memory") 1))
  (core instance $memory (instantiate $memory))
  (core func $increment (canon lower (func $increment) async
    (memory (core memory $memory "memory"))))
  (core func $trace (canon lower (func $trace)))
  (core func $return (canon task.return (result u32)))
  (core func $set.new (canon waitable-set.new))
  (core func $set.drop (canon waitable-set.drop))
  (core func $join (canon waitable.join))
  (core func $subtask.drop (canon subtask.drop))
  (core func $cancel (canon subtask.cancel async))
  (core module $guest
    (import "host" "memory" (memory 1))
    (import "host" "increment" (func $increment (param i32 i32) (result i32)))
    (import "host" "trace" (func $trace (param i32)))
    (import "host" "return" (func $return (param i32)))
    (import "host" "set.new" (func $set.new (result i32)))
    (import "host" "set.drop" (func $set.drop (param i32)))
    (import "host" "join" (func $join (param i32 i32)))
    (import "host" "subtask.drop" (func $subtask.drop (param i32)))
    (import "host" "cancel" (func $cancel (param i32) (result i32)))
    (global $set (mut i32) (i32.const 0))
    (global $subtask (mut i32) (i32.const 0))
    (func (export "enter") (param $value i32) (result i32)
      (local $status i32)
      (call $trace (i32.const 1))
      (global.set $set (call $set.new))
      (local.set $status (call $increment (local.get $value) (i32.const 0)))
      ;; The deliberately pending host future must yield STARTED, not RETURNED.
      (if (i32.ne (i32.and (local.get $status) (i32.const 15)) (i32.const 1))
        (then unreachable))
      (global.set $subtask (i32.shr_u (local.get $status) (i32.const 4)))
      ;; Cancellation must precede joining; -1 acknowledges an outstanding abort.
      (if (i32.ne (call $cancel (global.get $subtask)) (i32.const -1))
        (then unreachable))
      (call $join (global.get $subtask) (global.get $set))
      (i32.or (i32.const 2) (i32.shl (global.get $set) (i32.const 4)))) ;; WAIT
    (func (export "callback") (param $event i32) (param $handle i32)
          (param $status i32) (result i32)
      ;; Only the expected completed subtask may resume this continuation.
      (if (i32.ne (local.get $event) (i32.const 1)) (then unreachable))
      (if (i32.ne (local.get $handle) (global.get $subtask)) (then unreachable))
      (if (i32.ne (local.get $status) (i32.const 4)) (then unreachable))
      (call $trace (i32.const 2))
      (call $subtask.drop (global.get $subtask))
      (call $set.drop (global.get $set))
      ;; Marker confirms cancellation acknowledgment, not a completed import value.
      (call $return (i32.const 99))
      (call $trace (i32.const 3))
      (i32.const 0))) ;; EXIT
  (core instance $host
    (export "memory" (memory $memory "memory"))
    (export "increment" (func $increment))
    (export "trace" (func $trace))
    (export "return" (func $return))
    (export "set.new" (func $set.new))
    (export "set.drop" (func $set.drop))
    (export "join" (func $join))
    (export "subtask.drop" (func $subtask.drop))
    (export "cancel" (func $cancel)))
  (core instance $guest (instantiate $guest (with "host" (instance $host))))
  (func (export "run") async (param "value" u32) (result u32)
    (canon lift (core func $guest "enter") async
      (callback (core func $guest "callback")))))
