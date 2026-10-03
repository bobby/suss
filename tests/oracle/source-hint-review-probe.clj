;; Original development probe; run from tests/oracle against pinned analyzer.
(require '[cljs.analyzer :as ana] '[cljs.env :as env] 'cljs.core
         '[suss-oracle.declaration-macros :as declaration])
(def expressions
  [["scalar-false-hint" "scalar"]
   ["dynamic-scalar-false-hint" "*scalar*"]
   ["function-false-hint" "callable"]
   ["dynamic-function" "*callable*"]
   ["scalar-truthy-hint" "hinted"]
   ["dynamic-scalar-truthy-hint" "*hinted*"]
   ["invoke-function-false-hint" "(callable)"]
   ["invoke-dynamic-function" "(*callable*)"]
   ["local-false-hint" "(let [^{:tag false} x 7] x)"]
   ["parameter-false-hint" "(fn [^{:tag false} x] x)"]
   ["raw-return-metadata" "(raw-return 27)"]
   ["computed-return-overrides-raw" "(computed-return)"]
   ["provisional-var-tag" "provisional"]
   ["provisional-return-metadata" "(provisional)"]
   ["scalar-nil-hint" "nil-scalar"]
   ["function-nil-hint" "nil-callable"]
   ["provisional-nil-var-tag" "nil-provisional"]
   ["provisional-nil-return" "(nil-provisional)"]
   ["overlay-fn-var-false" "(disabled)"]
   ["overlay-return-false" "(false-return 32)"]
   ["overlay-return-nil" "(nil-return 33)"]
   ["overlay-var-tag-nil" "nil-overlay"]
   ["nested-nil-tag-return" "(fn [] nil-callable)"]])
(env/with-compiler-env (env/default-compiler-env)
  (binding [ana/*cljs-ns* 'hint-review ana/*cljs-file* "hint-review.cljs"]
    (swap! env/*compiler* assoc-in [::ana/namespaces 'hint-review]
      {:name 'hint-review :defs {}})
    (let [env (assoc (ana/empty-env) :ns (ana/get-namespace 'hint-review))]
      (doseq [source ["(def ^{:tag false} scalar 7)"
                      "(def ^{:dynamic true :tag false} *scalar* 8)"
                      "(def ^{:tag false} callable (fn [] 23))"
                      "(def ^:dynamic *callable* (fn [] 24))"
                      "(def ^string hinted 9)"
                      "(def ^{:dynamic true :tag string} *hinted* 10)"
                      "(def ^{:ret-tag string} raw-return (fn [x] x))"
                      "(def ^{:ret-tag string} computed-return (fn [] 25))"
                      "(def ^{:declared true :tag string :ret-tag boolean} provisional (fn [] 26))"
                      "(def ^{:tag nil} nil-scalar 29)"
                      "(def ^{:tag nil} nil-callable (fn [] 30))"
                      "(def ^{:declared true :tag nil :ret-tag nil} nil-provisional (fn [] 31))"
                      "(def ^{:top-fn {:fn-var false}} disabled (fn [] 32))"
                      "(def ^{:ret-tag string :top-fn {:ret-tag false}} false-return (fn [x] x))"
                      "(def ^{:ret-tag string :top-fn {:ret-tag nil}} nil-return (fn [x] x))"
                      "(def ^{:tag string :top-fn {:tag nil}} nil-overlay (fn [] 34))"]]
        (ana/analyze env (read-string source)))
      (let [rows (mapv (fn [[label source]]
                        (let [ast (ana/analyze env (read-string source))]
                          [label (contains? ast :tag) (declaration/data (:tag ast))
                           (contains? ast :inferred-ret-tag)
                           (declaration/data (:inferred-ret-tag ast))])) expressions)]
        (println (declaration/json rows))
        (spit "source-hint-review-observations.json"
          (str "{\"schema\":1,\"upstream\":\"c4295f303100bbf5afac449242d30bca1126f1a1\",\"cases\":"
            (declaration/json rows) "}\n"))))))
