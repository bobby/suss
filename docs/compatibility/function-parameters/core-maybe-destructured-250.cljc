(core/defn ^{:private true}
   maybe-destructured
     [params body]
     (if (every? core/symbol? params)
       (cons params body)
       (core/loop [params params
                   new-params (with-meta [] (meta params))
                   lets []]
         (if params
           (if (core/symbol? (first params))
             (recur (next params) (conj new-params (first params)) lets)
             (core/let [gparam (gensym "p__")]
               (recur (next params) (conj new-params gparam)
                 (core/-> lets (conj (first params)) (conj gparam)))))
           `(~new-params
              (let ~lets
                ~@body))))))
