(module
  (tag $value (param i32))
  (func (export "answer") (result i32)
    (block $done (result i32)
      (try_table (catch $value $done)
        (throw $value (i32.const 42)))
      unreachable)))
