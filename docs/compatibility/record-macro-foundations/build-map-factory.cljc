(core/defn- build-map-factory [rsym rname fields]
  (core/let [fn-name (with-meta (symbol (core/str 'map-> rsym))
                       (assoc (meta rsym) :factory :map))
             docstring (core/str "Factory function for " rname ", taking a map of keywords to field values.")
             ms (gensym)
             ks (map keyword fields)
             getters (map (core/fn [k] `(~k ~ms)) ks)]
    `(defn ~fn-name ~docstring [~ms]
       (let [extmap# (cond->> (dissoc ~ms ~@ks)
                        (record? ~ms) (into {}))]
         (new ~rname ~@getters nil (not-empty extmap#) nil)))))
