(module
  (type $box (struct (field i32)))
  (func (export "answer") (result i32)
    (struct.get $box 0 (struct.new $box (i32.const 42)))))
