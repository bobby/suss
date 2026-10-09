(core/defn- type-hint-multi-arity-sig
  [type-sym sig]
  (list* (type-hint-first-arg type-sym (first sig)) (next sig)))
