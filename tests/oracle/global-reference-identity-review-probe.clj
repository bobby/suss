;; Original independent review probe against the pinned analyzer, development only.
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(def observations (atom []))
(defmacro inspect-copy [a b]
  (let [left (get-in &env [:locals a :init])
        right (get-in &env [:locals b :init])
        info (:info left)
        old (get-in left [:env :ns :defs 'value])
        current (get-in &env [:ns :defs 'value])]
    (swap! observations conj [(identical? info (:info right))
      (identical? info old) (identical? (:meta info) (:meta old))
      (identical? (:meta info) (:meta current))
      (:doc info) (:ns old) (:doc current)
      (= (:name info) 'review-global/value) (= (:ns info) 'review-global)
      (contains? old :op)])
    nil))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review-global]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'review-global]
      {:name 'review-global :defs {} :use-macros {'inspect-copy 'user}})
    (doseq [source ["(def ^{:doc \"before\" :ns raw} value 17)"
                    "(let [a value b value changed (def ^{:doc \"after\"} value false)] (inspect-copy a b))"]]
      (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'review-global)) (read-string source)))))
(assert (= @observations '[[false false true true "before" raw "before" true true false]]) (pr-str @observations))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review-global]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'review-global]
      {:name 'review-global :defs {} :use-macros {'inspect-copy 'user}})
    (doseq [source ["(def ^{:top-fn {:meta {:custom true}}} value (fn [] 17))"
                    "(let [a value b value] (inspect-copy a b))"]]
      (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'review-global)) (read-string source)))))
(assert (= (second @observations) [false false true true nil nil nil true true false]) (pr-str @observations))
(prn @observations)
