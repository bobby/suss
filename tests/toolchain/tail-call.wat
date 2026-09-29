(module
  (func $answer (result i32) (i32.const 42))
  (func (export "answer") (result i32) (return_call $answer)))
