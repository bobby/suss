;; Original M0-02 canonical endpoint transfer fixture, not Suss-generated code.
;; Transfers owned read ends through a core guest without implicitly awaiting
;; a future. Payload production/consumption is independently checked by Rust.
(component
  (type $future (future u32))
  (type $stream (stream u8))
  (core module $guest
    (func (export "echo") (param i32) (result i32) (local.get 0)))
  (core instance $guest (instantiate $guest))
  (func (export "future") (param "value" $future) (result $future)
    (canon lift (core func $guest "echo")))
  (func (export "stream") (param "value" $stream) (result $stream)
    (canon lift (core func $guest "echo"))))
