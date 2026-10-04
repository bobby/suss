(ns suss-oracle.control-source-asts
  (:require [suss-oracle.collection-source-asts :as source]
            [suss-oracle.declaration-macros :as declaration]))

;; Original development-only projection, following genuine declared child edges.
;; Reuse the bounded source-node projection without reanalyzing any form.
(defmacro observe [label binding]
  (let [row [label (source/project (get-in &env [:locals binding :init]) 0)]]
    (spit "out/control-source-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
