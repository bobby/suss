(core/defn- add-proto-methods* [pprefix type type-sym [f & meths :as form]]
  (core/let [pf (core/str pprefix (munge (name f)))]
    (if (vector? (first meths))
      ;; single method case
      (core/let [meth meths]
        [`(set! ~(extend-prefix type-sym (core/str pf "$arity$" (count (first meth))))
            ~(with-meta `(fn ~@(adapt-proto-params type meth)) (meta form)))])
      (map (core/fn [[sig & body :as meth]]
             `(set! ~(extend-prefix type-sym (core/str pf "$arity$" (count sig)))
                ~(with-meta `(fn ~(adapt-proto-params type meth)) (meta form))))
        meths))))
