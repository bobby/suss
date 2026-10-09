(core/defn- collect-protocols [impls env]
  (core/->> impls
      (filter core/symbol?)
      (map #(:name (cljs.analyzer/resolve-var (dissoc env :locals) %)))
      (into #{})))
