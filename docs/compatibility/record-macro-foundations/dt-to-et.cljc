(core/defn dt->et
  ([type specs fields]
   (dt->et type specs fields false))
  ([type specs fields inline]
   (core/let [annots {:cljs.analyzer/type type
                      :cljs.analyzer/protocol-impl true
                      :cljs.analyzer/protocol-inline inline}]
     (core/loop [ret [] specs specs]
       (if (seq specs)
         (core/let [p     (first specs)
                    ret   (core/-> (conj ret p)
                            (into (reduce (partial annotate-specs annots) []
                                    (group-by first (take-while seq? (next specs))))))
                    specs (drop-while seq? (next specs))]
           (recur ret specs))
         ret)))))
