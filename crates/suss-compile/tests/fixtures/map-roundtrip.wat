;; M0-02 engine/canonical ABI fixture, not Suss-generated adapters.
(component
  (type $map (map string u32))
  (type $echo-type (func (param "values" $map) (result $map)))
  (core module $storage
    (memory (export "memory") 1)
    (global $heap (mut i32) (i32.const 16))
    ;; Bounded fixture allocator: canonical lowering only allocates fresh data.
    ;; Reject reallocations instead of pretending to implement production realloc.
    (func (export "realloc") (param $old i32) (param $old-size i32)
          (param $align i32) (param $size i32) (result i32)
      (local $ptr i32)
      (if (i32.or (local.get $old) (local.get $old-size)) (then unreachable))
      (local.set $ptr
        (i32.and
          (i32.add (global.get $heap) (i32.sub (local.get $align) (i32.const 1)))
          (i32.sub (i32.const 0) (local.get $align))))
      (global.set $heap (i32.add (local.get $ptr) (local.get $size)))
      (if (i32.gt_u (global.get $heap) (i32.const 65536)) (then unreachable))
      (local.get $ptr))
    (func (export "echo") (param $ptr i32) (param $len i32) (result i32)
      ;; Canonical result area is a pointer/length pair, separate from input data.
      (i32.store (i32.const 0) (local.get $ptr))
      (i32.store (i32.const 4) (local.get $len))
      (i32.const 0)))
  (core instance $storage (instantiate $storage))
  (func $echo (type $echo-type)
    (canon lift (core func $storage "echo")
      (memory (core memory $storage "memory")) (realloc (core func $storage "realloc"))))
  (export "echo" (func $echo)))
