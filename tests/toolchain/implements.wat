(component
  (type $logger (instance (export "log" (func (param "message" string)))))
  (import "primary" (implements "probe:runtime/logger") (instance (type $logger))))
