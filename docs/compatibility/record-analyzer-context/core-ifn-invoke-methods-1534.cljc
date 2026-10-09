(core/defn- ifn-invoke-methods [type type-sym [f & meths :as form]]
  (map
    (core/fn [meth]
      (core/let [arity (count (first meth))]
        `(set! ~(extend-prefix type-sym (symbol (core/str "cljs$core$IFn$_invoke$arity$" arity)))
           ~(with-meta `(fn ~meth) (meta form)))))
    (map #(adapt-ifn-invoke-params type %) meths)))
