(core/defn- base-assign-impls [env resolve tsym type [p sigs]]
  (update-protocol-var p tsym env)
  (core/let [psym       (resolve p)
             pfn-prefix (subs (core/str psym) 0
                          (clojure.core/inc (.indexOf (core/str psym) "/")))]
    (cons `(unchecked-set ~psym ~type true)
      (map (core/fn [[f & meths :as form]]
             `(unchecked-set ~(symbol (core/str pfn-prefix f))
                ~type ~(with-meta `(fn ~@meths) (meta form))))
        sigs))))
