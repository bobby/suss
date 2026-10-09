(core/defmacro exists?
  "Return true if argument exists, analogous to usage of typeof operator
   in JavaScript."
  [x]
  (if (core/symbol? x)
    (core/let [x     (core/cond-> (:name (cljs.analyzer/resolve-var &env x))
                       (= "js" (namespace x)) name)
               segs  (string/split (core/str (string/replace-first (core/str x) "/" ".")) #"\.")
               n     (count segs)
               syms  (map
                       #(vary-meta (symbol "js" (string/join "." %))
                          assoc :cljs.analyzer/no-resolve true)
                       (reverse (take n (iterate butlast segs))))
               js    (string/join " && " (repeat n "(typeof ~{} !== 'undefined')"))]
      (bool-expr (concat (core/list 'js* js) syms)))
    `(some? ~x)))
