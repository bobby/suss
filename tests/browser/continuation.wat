;; Feasibility fixture: a rooted GC continuation outlives a host callback.
;; This is not the production Suss ABI or the canonical Component Model ABI.
(module
  (import "bridge" "schedule" (func $schedule (param i32)))
  (type $frame (struct (field i32)))
  (global $pending (mut (ref null $frame)) (ref.null $frame))
  (func (export "begin") (param $base i32) (param $delta i32)
    (global.set $pending (struct.new $frame (local.get $base)))
    (call $schedule (local.get $delta)))
  (func (export "resume") (param $delta i32) (result i32) (local $result i32)
    (local.set $result (i32.add (struct.get $frame 0 (global.get $pending)) (local.get $delta)))
    (global.set $pending (ref.null $frame))
    (local.get $result))
  (func (export "cancel") (global.set $pending (ref.null $frame)))
  (func (export "pending") (result i32) (i32.eqz (ref.is_null (global.get $pending)))))
