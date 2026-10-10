(core/defn- type-hint-single-arity-sig
  [type-sym sig]
  (list* (first sig) (type-hint-first-arg type-sym (second sig)) (nnext sig)))
