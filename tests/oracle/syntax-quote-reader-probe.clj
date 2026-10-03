;; Original development-only probe. Read through the actual pinned ClojureScript
;; analyzer source reader, including its resolver and alias binding. Printed
;; observations are not native execution or a substitute for Node evaluation.
(require 'cljs.core)
(require '[cljs.analyzer :as ana] '[cljs.env :as env]
         '[suss-oracle.declaration-macros :as declaration])

(binding [env/*compiler* (env/default-compiler-env)
          ana/*cljs-ns* 'user]
  (ana/analyze (ana/empty-env) '(ns user))
  (doseq [[label source]
          [["add-one" (slurp "syntax-quote-add-one.sus")]
           ["sum-inputs" (slurp "syntax-quote-sum-inputs.sus")]
           ["twice" (slurp "syntax-quote-twice.sus")]
           ["quoted-data" (slurp "syntax-quote-data.sus")]
           ["quoted-vector" (slurp "syntax-quote-vector-splice.sus")]
           ["scalars" "`[nil false true 42 \"text\" :key]"]
           ["empty-list" "`()"]
           ["vector-splice" "`[1 ~x ~@xs]"]
           ["map" "`{:a ~x :b 2}"]
           ["set" "`#{~x :a}"]
           ["metadata" "`^{:marker true} [~x]"]
           ["nested" "`(outer `(inner value#) value#)"]]]
    (let [forms (vec (ana/forms-seq* (java.io.StringReader. source)))]
      (assert (= 1 (count forms)))
      (println (declaration/json [label (declaration/data (first forms))])))))
