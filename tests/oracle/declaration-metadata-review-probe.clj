; Original development-only probe against the pinned analyzer.
; Run from tests/oracle: clojure -Srepro -M declaration-metadata-review-probe.clj
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core)
(defmacro pr160-provisional []
  (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'review :defs 'pending]) [:name :meta])]
    (assert (= actual '{:name review/pending :meta [:raw]}) (pr-str actual))
    (prn :provisional actual))
  0)
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review ana/*cljs-file* "review.cljs"]
    (doseq [s ["(def ^:dynamic f (fn [] 7))" "(def ^{:tag false} g (fn [] 7))" "(def ^{:tag false} scalar 7)" "(def ^{:name custom :test true} raw 7)" "(def ^{:top-fn {:name replaced :private false :doc \"override\" :meta {:test :replacement} :ret-tag wrong}} overlay (fn [] 7))"]]
      (let [f (read-string s) e (assoc (ana/empty-env) :ns {:name 'review :defs {}})]
        (ana/analyze e f)
        (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'review :defs (second f)]) [:name :tag :ret-tag :private :doc :meta])
              expected (get '{f {:name review/f :ret-tag number :meta {:dynamic true :file nil}}
                              g {:name review/g :tag false :ret-tag number :meta {:tag false :file nil}}
                              scalar {:name review/scalar :tag number :meta {:tag false :file nil}}
                              raw {:name custom :tag number :meta {:name custom :file nil}}
                              overlay {:name replaced :ret-tag number :private false :doc "override" :meta {:test :replacement}}}
                            (second f))]
          (assert (= actual expected) (pr-str [s actual expected]))
          (prn s actual))))))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {} :use-macros {'pr160-provisional 'user}})
      (read-string "(def ^{:meta [:raw]} pending (pr160-provisional))"))))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'cljs.core ana/*cljs-file* "native-core.sus"]
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'cljs.core :defs {}}) '(def core-probe 7))
    (let [actual (select-keys (get-in @env/*compiler* [::ana/namespaces 'cljs.core :defs 'core-probe]) [:file :meta])]
      (assert (= actual {:file "cljs/core.cljs" :meta {:file "cljs/core.cljs"}}) (pr-str actual))
      (prn :core actual))))

;; Explicit def docstrings are published only on completion, not in the
;; provisional record observed during initializer analysis.
(def document-observations (atom []))
(defmacro pr160-documents []
  (let [snapshot (get-in &env [:ns :defs])
        catalog (:defs (ana/get-namespace 'review))
        row [(contains? snapshot 'tracked)
             (:doc (get snapshot 'tracked))
             (:doc (get catalog 'tracked))]]
    (swap! document-observations conj row)
    0))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'review]
      {:name 'review :defs {} :use-macros {'pr160-documents 'user}})
    (doseq [source ["(def tracked \"first\" (pr160-documents))"
                    "(def tracked \"second\" (pr160-documents))"
                    "(pr160-documents)"]]
      (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace 'review))
        (read-string source)))))
(assert (= @document-observations [[false nil nil] [true "first" nil] [true "second" "second"]])
  (pr-str @document-observations))
(prn :documents @document-observations)

;; Raw function-looking metadata remains data on a scalar. Empty top-fn
;; contributes no computed arity group, leaving existing symbol metadata.
(def function-field-keys [:fn-var :variadic? :max-fixed-arity :method-params :arglists :arglists-meta])
(def function-observations (atom []))
(defn function-fields [info]
  (mapv (fn [key] [(contains? info key) (get info key)]) function-field-keys))
(defmacro pr160-function-fields [name]
  (let [row (function-fields (get-in @env/*compiler* [::ana/namespaces 'review :defs name]))]
    (swap! function-observations conj row)
    0))
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (doseq [source ["(def ^{:fn-var true :variadic? true :max-fixed-arity 9 :method-params [[given]] :arglists [[given]] :arglists-meta [nil]} raw-scalar 7)"
                    "(def ^{:fn-var false :variadic? true :max-fixed-arity 9 :method-params [[given]] :arglists [[given]] :arglists-meta [nil] :top-fn {}} raw-function (fn [x] x))"
                    "(def ^{:fn-var false :variadic? true :max-fixed-arity 9 :method-params [[given]] :arglists-meta [nil]} normal-function (fn [x] x))"]]
      (let [form (read-string source)]
        (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {}}) form)
        (let [actual (function-fields (get-in @env/*compiler* [::ana/namespaces 'review :defs (second form)]))
              expected (if (= (second form) 'normal-function)
                         '[[true true] [true false] [true 1] [true ([x])] [true nil] [true ()]]
                         '[[true true] [true true] [true 9] [true [[given]]] [true [[given]]] [true [nil]]])]
          (assert (= actual expected) (pr-str [source actual expected]))
          (prn :function-fields source actual))))
    (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {} :use-macros {'pr160-function-fields 'user}})
      (read-string "(def ^{:fn-var true :variadic? true :max-fixed-arity 9 :method-params [[given]] :arglists [[given]] :arglists-meta [nil]} staged (fn [x] (pr160-function-fields staged)))"))))
(assert (= @function-observations '[[[true true] [true true] [true 9] [true [[given]]] [true [[given]]] [true [nil]]]])
  (pr-str @function-observations))
(prn :provisional-function-fields @function-observations)

;; Fresh forward declarations derive portable method parameters from arglists,
;; while still having no source initializer or source callable.
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'review]
    (doseq [[source expected] [["(def ^{:declared :marker :fn-var false :method-params [[old]] :arglists '([given])} typed)"
                               '[[true true] [false nil] [false nil] [true ([given])] [true (quote ([given]))] [false nil]]]
                              ["(def ^{:declared true :arglists [[given] [more]]} typed-vector)"
                               '[[true true] [false nil] [false nil] [true [more]] [true [[given] [more]]] [false nil]]]
                              ["(def ^{:declared true :arglists {:a [x] :b [y]}} typed-map)"
                               '[[true true] [false nil] [false nil] [true [:b [y]]] [true {:a [x] :b [y]}] [false nil]]]]]
      (let [form (read-string source)]
        (ana/analyze (assoc (ana/empty-env) :ns {:name 'review :defs {}}) form)
        (let [info (get-in @env/*compiler* [::ana/namespaces 'review :defs (second form)])
              actual (function-fields info)]
          (assert (= actual expected) (pr-str [source actual expected]))
          (assert (true? (:declared info)) (pr-str info))
          (prn :declared-function-fields source actual (:declared info)))))))
