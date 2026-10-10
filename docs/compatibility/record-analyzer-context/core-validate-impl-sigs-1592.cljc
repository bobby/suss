(core/defn- validate-impl-sigs [env p method]
  (core/when-not (= p 'Object)
    (core/let [var (ana/resolve-var (dissoc env :locals) p)
               minfo (core/-> var :protocol-info :methods)
               method-name (first method)
               ->name (comp symbol name)
               [fname sigs] (if (core/vector? (second method))
                              [(->name method-name) [(second method)]]
                              [(->name method-name) (map first (rest method))])
               decmeths (core/get minfo fname ::not-found)]
      (core/when (= decmeths ::not-found)
        (ana/warning :protocol-invalid-method env {:protocol p :fname fname :no-such-method true}))
      (core/when (namespace method-name)
        (core/let [method-var (ana/resolve-var (dissoc env :locals) method-name
                                ana/confirm-var-exist-warning)]
          (core/when-not (= (:name var) (:protocol method-var))
            (ana/warning :protocol-invalid-method env
              {:protocol p :fname method-name :no-such-method true}))))
      (core/loop [sigs sigs seen #{}]
        (core/when (seq sigs)
          (core/let [sig (first sigs)
                     c   (count sig)]
            (core/when (contains? seen c)
              (ana/warning :protocol-duped-method env {:protocol p :fname fname}))
            (core/when (some '#{&} sig)
              (ana/warning :protocol-impl-with-variadic-method env {:protocol p :name fname}))
            (core/when (core/and (not= decmeths ::not-found) (not (some #{c} (map count decmeths))))
              (ana/warning :protocol-invalid-method env {:protocol p :fname fname :invalid-arity c}))
            (recur (next sigs) (conj seen c))))))))
