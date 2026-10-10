(core/defn- type-hint-multi-arity-sigs
  [type-sym sigs]
  (list* (first sigs) (map (partial type-hint-multi-arity-sig type-sym) (rest sigs))))
