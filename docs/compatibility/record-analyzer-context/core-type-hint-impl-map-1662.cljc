(core/defn- type-hint-impl-map
  [type-sym impl-map]
  (reduce-kv (core/fn [m proto sigs]
               (assoc m proto (map (partial type-hint-sigs type-sym) sigs)))
    {} impl-map))
