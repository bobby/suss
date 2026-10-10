(core/defn- update-protocol-var [p type env]
  (core/when-not (= 'Object p)
    (core/if-let [var (cljs.analyzer/resolve-existing-var (dissoc env :locals) p)]
      (do
        (core/when-not (:protocol-symbol var)
          (cljs.analyzer/warning :invalid-protocol-symbol env {:protocol p}))
        (core/when (core/and (:protocol-deprecated cljs.analyzer/*cljs-warnings*)
                (core/-> var :deprecated)
                (not (core/-> p meta :deprecation-nowarn)))
          (cljs.analyzer/warning :protocol-deprecated env {:protocol p}))
        (core/when (:protocol-symbol var)
          (swap! env/*compiler* update-in [:cljs.analyzer/namespaces]
            (core/fn [ns]
              (update-in ns [(:ns var) :defs (symbol (name p)) :impls]
                conj type)))))
      (core/when (:undeclared cljs.analyzer/*cljs-warnings*)
        (cljs.analyzer/warning :undeclared-protocol-symbol env {:protocol p})))))
