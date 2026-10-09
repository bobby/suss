(core/defn- resolve-var [env sym]
  (core/let [ret (:name (cljs.analyzer/resolve-var env sym))]
    (core/assert ret (core/str "Can't resolve: " sym))
    ret))
