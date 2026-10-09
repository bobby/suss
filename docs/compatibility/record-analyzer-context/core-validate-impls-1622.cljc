(core/defn- validate-impls [env impls]
  (core/loop [protos #{} impls impls]
    (core/when (seq impls)
      (core/let [proto   (first impls)
                 methods (take-while seq? (next impls))
                 impls   (drop-while seq? (next impls))]
        (core/when (contains? protos proto)
          (ana/warning :protocol-multiple-impls env {:protocol proto}))
        (core/loop [seen #{} methods methods]
          (core/when (seq methods)
            (core/let [[fname :as method] (first methods)]
              (core/when (contains? seen fname)
                (ana/warning :extend-type-invalid-method-shape env
                  {:protocol proto :method fname}))
              (validate-impl-sigs env proto method)
              (recur (conj seen fname) (next methods)))))
        (recur (conj protos proto) impls)))))
