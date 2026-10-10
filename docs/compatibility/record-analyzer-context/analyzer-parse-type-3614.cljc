(defn parse-type
  [op env [_ tsym fields pmasks body :as form]]
  (let [t (:name (resolve-var (dissoc env :locals) tsym))
        locals (reduce (fn [m fld]
                         (assoc m fld
                                {:name fld
                                 :line (get-line fld env)
                                 :column (get-col fld env)
                                 :local :field
                                 :field true
                                 :mutable (-> fld meta :mutable)
                                 :unsynchronized-mutable (-> fld meta :unsynchronized-mutable)
                                 :volatile-mutable (-> fld meta :volatile-mutable)
                                 :tag (-> fld meta :tag)
                                 :shadow (m fld)}))
                       {} (if (= :defrecord op)
                            (concat fields '[__meta __extmap ^:mutable __hash])
                            fields))
        protocols (-> tsym meta :protocols)]
    (swap! env/*compiler* update-in [::namespaces (-> env :ns :name) :defs tsym]
           (fn [m]
             (let [m (assoc (or m {})
                       :name t
                       :tag 'function
                       :type true
                       :num-fields (count fields)
                       :record (= :defrecord op))]
               (merge m
                      (dissoc (meta tsym) :protocols)
                      {:protocols protocols}
                      (source-info tsym env)))))
    {:op op :env env :form form :t t :fields fields :pmasks pmasks
     :tag 'function
     :protocols (disj protocols 'cljs.core/Object)
     :children [#_:fields :body]
     :body (analyze (assoc env :locals locals) body)}))
