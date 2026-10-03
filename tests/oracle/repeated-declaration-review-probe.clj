;; Original development-only observation against the pinned analyzer.
;; Run from tests/oracle with clojure -Srepro -M repeated-declaration-review-probe.clj.
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(def fields [:name :ns :private :dynamic :doc :declared :tag :ret-tag
             :fn-var :variadic? :max-fixed-arity :method-params
             :arglists :arglists-meta :meta :line :column :file])
(defn entry [name]
  (select-keys (get-in @env/*compiler* [::ana/namespaces 'declaration-review :defs name]) fields))
(defn analyze [source]
  (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'declaration-review))
    (read-string source)))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'declaration-review ana/*cljs-file* "declaration-review.cljs"]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'declaration-review]
      {:name 'declaration-review :defs {}})
    (analyze "(def ^:private scalar \"scalar-doc\" 7)")
    (analyze "(def ^:private callable \"callable-doc\" (fn [x] (+ x 1)))")
    (let [scalar (entry 'scalar) callable (entry 'callable)]
      ;; Expanded forms of pinned core.cljc175–176 declare, whose symbols
      ;; retain their own metadata and additionally carry :declared true.
      (analyze "(def ^{:declared true} forward)")
      (analyze "(def ^{:declared true :private false :doc \"replacement\" :tag string} scalar)")
      (analyze "(def ^{:declared true :private false :doc \"replacement\" :fn-var false} callable)")
      (analyze "(def ^{:declared :marker :doc \"ignored\"} callable)")
      (let [sym (vary-meta (read-string "^{:declared false :doc \"ignored\"} callable") assoc :declared true)]
        (assert (true? (:declared (meta sym))))
        (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'declaration-review)) (list 'def sym)))
      (assert (= scalar (entry 'scalar)) (pr-str [scalar (entry 'scalar)]))
      (assert (= callable (entry 'callable)) (pr-str [callable (entry 'callable)]))
      (assert (= (:doc scalar) "scalar-doc"))
      (assert (true? (:private scalar)))
      (assert (true? (:fn-var callable)))
      (prn :completed-unchanged scalar callable))
    (let [forward (entry 'forward)]
      (analyze "(def ^{:declared true :doc \"changed-forward\" :tag number} forward)")
      (assert (= forward (entry 'forward)) (pr-str [forward (entry 'forward)]))
      (assert (true? (:declared forward)))
      (prn :forward-unchanged forward))
    (doseq [name '[ordinary false-record nil-record initialized]]
      (analyze (str "(def " name " \"old\" 1)")))
    (analyze "(def ordinary)")
    (analyze "(def ^{:declared false :doc \"new\"} false-record)")
    (analyze "(def ^{:declared nil :doc \"new\"} nil-record)")
    (analyze "(def ^{:declared :marker} initialized \"new\" 9)")
    (assert (not (contains? (entry 'ordinary) :doc)))
    (prn :replacement-boundaries (mapv entry '[ordinary false-record nil-record initialized]))
    (doseq [name '[false-record nil-record]]
      (assert (= "new" (:doc (entry name)))))
    (assert (not (contains? (entry 'initialized) :doc)))
    (assert (false? (:declared (entry 'false-record))))
    (assert (nil? (:declared (entry 'nil-record))))
    (prn :replacement-boundaries (mapv entry '[ordinary false-record nil-record initialized]))))
