(component
  (type $completion (future u32))
  (type $chunks (stream u8))
  (import "completion" (func (result $completion)))
  (import "chunks" (func (result $chunks))))
