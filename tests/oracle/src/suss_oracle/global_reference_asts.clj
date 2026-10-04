(ns suss-oracle.global-reference-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original projection of the initializer already analyzed by the compiler.
;; Resolve-existing-var performs the declaration revision capture; do not
;; analyze the source again or infer a var AST from its runtime value.
(defmacro observe [label binding catalog-name]
  (let [ast (get-in &env [:locals binding :init])
        info (:info ast)
        catalog (get-in ast [:env :ns :defs catalog-name])
        field (fn [record key]
                [(name key) (contains? record key)
                 (declaration/data (get record key))])
        row [label
             [(field ast :op) (field ast :name) (field ast :ns) (field ast :tag)]
             [(field info :op) (field info :name) (field info :ns) (field info :tag) (field info :doc) (field info :declared) (field info :dynamic) (field info :fn-var) (field info :ret-tag)]
             [(field catalog :op) (field catalog :ns)]
             [(contains? ast :val) (contains? ast :children)
              (= (:name ast) (:name info)) (= (:ns ast) (:ns info))]]]
    (spit "out/global-reference-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
