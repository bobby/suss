;; Original development-only fixture: invoke the pinned analyzer's macroexpander
;; on forms from its indexing reader. This does not certify native execution.
(require 'cljs.core)
(require '[cljs.analyzer :as ana] '[cljs.env :as env]
         '[cljs.vendor.clojure.tools.reader :as reader]
         '[cljs.vendor.clojure.tools.reader.reader-types :as readers]
         '[suss-oracle.declaration-macros :as declaration])

(let [macro-ns (create-ns 'suss-oracle.form-source-metadata)]
  (binding [*ns* macro-ns]
    (clojure.core/refer 'clojure.core)
    (load-string (slurp "form-source-metadata-macro.sus"))
    (load-string (slurp "form-source-metadata-generator.sus"))
    (load-string (slurp "form-source-metadata-tag-macro.sus"))))

(binding [env/*compiler* (env/default-compiler-env)]
  (let [e (-> (ana/empty-env)
              (assoc-in [:ns :use-macros 'form-source-facts]
                        'suss-oracle.form-source-metadata)
              (assoc-in [:ns :use-macros 'make-source-form]
                        'suss-oracle.form-source-metadata)
              (assoc-in [:ns :use-macros 'form-tag-facts]
                        'suss-oracle.form-source-metadata))]
    (doseq [[label source filename]
            [["nested" (slurp "form-source-metadata-input.sus") nil]
             ["unicode-crlf" (slurp "form-source-metadata-unicode.sus") nil]
             ["overrides" (slurp "form-source-metadata-overrides.sus") nil]
             ["file" (slurp "form-source-metadata-input.sus") "fixture/app.sus"]
             ["generated" "\n  (make-source-form)" nil]
             ["conditional" (slurp "form-source-metadata-conditional.sus") nil]
             ["chained" (slurp "form-source-metadata-chained.sus") nil]
             ["slash" (slurp "form-source-metadata-slash.sus") nil]
             ["tag" (slurp "form-source-metadata-tag.sus") nil]]]
      (let [input (readers/indexing-push-back-reader source 1 filename)
            parsed (reader/read {:read-cond :allow :features #{:cljs}} input)
            form (if (= 'do (first parsed)) (nth parsed 2) parsed)
            form (if (= label "generated") (ana/macroexpand-1 e form) form)
            expansion (ana/macroexpand-1 e form)]
        (assert (= 'quote (first expansion)))
        (println (declaration/json [label (second expansion)]))))))
