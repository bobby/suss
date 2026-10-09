(core/defn- type-hint-sigs
  [type-sym sig]
  (if (vector? (second sig))
    (type-hint-single-arity-sig type-sym sig)
    (type-hint-multi-arity-sigs type-sym sig)))
