(core/defn- prepare-protocol-masks [env impls]
  (core/let [resolve  (partial resolve-var env)
             impl-map (->impl-map impls)
             fpp-pbs  (seq
                        (keep fast-path-protocols
                          (map resolve
                            (keys impl-map))))]
    (if fpp-pbs
      (core/let [fpps  (into #{}
                         (filter (partial contains? fast-path-protocols)
                           (map resolve (keys impl-map))))
                 parts (core/as-> (group-by first fpp-pbs) parts
                         (into {}
                           (map (juxt key (comp (partial map peek) val))
                             parts))
                         (into {}
                           (map (juxt key (comp (partial reduce core/bit-or) val))
                             parts)))]
        [fpps (reduce (core/fn [ps p] (update-in ps [p] (core/fnil identity 0)))
                parts
                (range fast-path-protocol-partitions-count))]))))
