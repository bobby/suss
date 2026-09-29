;; Original optional Jco packaging probe, not compiled Suss output.
(component
  (core module $guest
    (type $box (struct (field i32)))
    (func (export "answer") (result i32)
      (struct.get $box 0 (struct.new $box (i32.const 42)))))
  (core instance $guest (instantiate $guest))
  (func (export "answer") (result u32)
    (canon lift (core func $guest "answer"))))
