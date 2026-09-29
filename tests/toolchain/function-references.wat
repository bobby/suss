(module
  (type $result (func (result i32)))
  (func $answer (type $result) (i32.const 42))
  (elem declare func $answer)
  (func (export "answer") (result i32) (call_ref $result (ref.func $answer))))
