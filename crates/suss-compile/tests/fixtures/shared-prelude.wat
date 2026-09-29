;; Prototype for ABI v1 feasibility only, not a published runtime ABI.
;; Identical recursive groups must canonicalize across independently loaded modules.
(rec
  (type $args (array (mut (ref null eq))))
  (type $invoke (func (param (ref null eq) (ref $args)) (result (ref null eq))))
  (type $closure (struct (field (ref null eq)) (field (ref $invoke))))
  (type $descriptor (struct (field i32)))
  (type $object (struct (field (ref $descriptor)) (field (ref $args))))
)
