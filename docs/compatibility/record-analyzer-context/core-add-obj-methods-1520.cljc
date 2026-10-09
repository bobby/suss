(core/defn- add-obj-methods [type type-sym sigs]
  (core/->> sigs
    ;; Elide all toString methods in :lite-mode
    (remove
      (core/fn [[f]]
        (core/and (ana/elide-to-string?) (core/= 'toString f))))
    (map
      (core/fn [[f & meths :as form]]
        (core/let [[f meths] (if (vector? (first meths))
                               [f [(rest form)]]
                               [f meths])]
          `(set! ~(extend-prefix type-sym f)
             ~(with-meta `(fn ~@(map #(adapt-obj-params type %) meths)) (meta form))))))))
