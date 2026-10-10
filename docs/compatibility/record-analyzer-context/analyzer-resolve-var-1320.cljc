(defn resolve-var
  "Resolve a var. Accepts a side-effecting confirm fn for producing
   warnings about unresolved vars."
  ([env sym]
   (resolve-var env sym nil))
  ([env sym confirm]
   (resolve-var env sym confirm true))
  ([env sym confirm default?]
   (let [locals (:locals env)]
     (if #?(:clj  (= "js" (namespace sym))
            :cljs (identical? "js" (namespace sym)))
       (let [symn (-> sym name symbol)
             shadowed-by-local (handle-symbol-local symn (get locals symn))]
         (cond
           (some? shadowed-by-local)
           (do (warning :js-shadowed-by-local env {:name sym})
               (assoc shadowed-by-local :op :local))

           :else
           (let [pre (->> (string/split (name sym) #"\.") (map symbol) vec)
                 res (resolve-extern (->> (string/split (name sym) #"\.") (map symbol) vec))]
             (when (and (not res)
                        ;; ignore exists? usage
                        (not (-> sym meta ::no-resolve)))
               (swap! env/*compiler* update-in
                 (into [::namespaces (-> env :ns :name) :externs] pre) merge {}))
             (merge
               {:name sym
                :op :js-var
                :ns   'js
                :tag  (with-meta (or (js-tag pre) (:tag (meta sym)) 'js)
                        {:prefix pre
                         :ctor   (-> res :info :ctor)})}
               (when-let [ret-tag (js-tag pre :ret-tag)]
                 {:js-fn-var true
                  :ret-tag   ret-tag})))))
       (let [s  (str sym)
             lb (handle-symbol-local sym (get locals sym))
             current-ns (-> env :ns :name)]
         (cond
           (some? lb) (assoc lb :op :local)

           (some? (namespace sym))
           (let [ns (namespace sym)]
             (if-let [resolved (and (nil? (resolve-ns-alias env ns nil))
                                    (not (dotted-symbol? ns))
                                    (resolve-var env (symbol ns) nil false)
                                    (resolve-var env (qualified->dotted sym) nil false))]
               resolved
               (let [ns      (if #?(:clj  (= "clojure.core" ns)
                                    :cljs (identical? "clojure.core" ns))
                               "cljs.core"
                               ns)
                     full-ns (resolve-ns-alias env ns
                               (or (and (js-module-exists? ns)
                                        (gets @env/*compiler* :js-module-index ns :name))
                                   (symbol ns)))]
                 (when (some? confirm)
                   (when (not= current-ns full-ns)
                     (confirm-ns env full-ns))
                   (confirm env full-ns (symbol (name sym))))
                 (resolve* env sym full-ns current-ns))))

           (dotted-symbol? sym)
           (let [idx    (.indexOf s ".")
                 prefix (symbol (subs s 0 idx))
                 suffix (subs s (inc idx))]
             ;; check if prefix is some existing def
             (if-let [resolved (resolve-var env prefix nil false)]
               (update resolved :name #(symbol (str % "." suffix)))
               ;; glib imports (i.e. (:import [goog.module ModuleLoader])
               ;; are always just dotted symbols after the recursion
               (let [s   (str
                           (cond->> s
                             (goog-module-dep? sym)
                             (resolve-import env)))
                     idx (.lastIndexOf (str s) ".")
                     pre (subs s 0 idx)
                     suf (subs s (inc idx))]
                 {:op   :var
                  :name (symbol pre suf)
                  :ns   (symbol pre)})))

           (some? (gets @env/*compiler* ::namespaces current-ns :uses sym))
           (let [full-ns (gets @env/*compiler* ::namespaces current-ns :uses sym)]
             (resolve* env sym full-ns current-ns))

           (some? (gets @env/*compiler* ::namespaces current-ns :renames sym))
           (let [qualified-symbol (gets @env/*compiler* ::namespaces current-ns :renames sym)
                 full-ns (symbol (namespace qualified-symbol))
                 sym     (symbol (name qualified-symbol))]
             (resolve* env sym full-ns current-ns))

           (some? (gets @env/*compiler* ::namespaces current-ns :imports sym))
           (recur env (gets @env/*compiler* ::namespaces current-ns :imports sym) confirm default?)

           (some? (gets @env/*compiler* ::namespaces current-ns :defs sym))
           (do
             (when (some? confirm)
               (confirm env current-ns sym))
             (merge (gets @env/*compiler* ::namespaces current-ns :defs sym)
               {:name (symbol (str current-ns) (str sym))
                :op :var
                :ns current-ns}))

           (core-name? env sym)
           (let [sym (resolve-alias 'cljs.core sym)]
             (when (some? confirm)
               (confirm env 'cljs.core sym))
             (merge (gets @env/*compiler* ::namespaces 'cljs.core :defs sym)
               {:name (symbol "cljs.core" (str sym))
                :op :var
                :ns 'cljs.core}))

           (invokeable-ns? s env)
           (resolve-invokeable-ns s current-ns env)

           :else
           (when default?
             (when (some? confirm)
               (confirm env current-ns sym))
             (merge (gets @env/*compiler* ::namespaces current-ns :defs sym)
               {:name (symbol (str current-ns) (str sym))
                :op :var
                :ns current-ns}))))))))
