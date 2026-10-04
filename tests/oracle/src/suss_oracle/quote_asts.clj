(ns suss-oracle.quote-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Inspect already analyzed syntax, never analyze/evaluate the quoted datum.
(defmacro observe [label binding]
  (let [ast (get-in &env [:locals binding :init])
        expr (:expr ast)
        field (fn [record key]
                [(name key) (contains? record key)
                 (declaration/data (get record key))])
        row [label
             [(field ast :op) (field ast :literal?) (field ast :form)
              (field ast :tag) (field ast :children) (field ast :val)]
             [(field expr :op) (field expr :literal?) (field expr :form)
              (field expr :tag) (field expr :val) (field expr :children)]
             [(= (:form expr) (:val expr)) (= (:env ast) (:env expr))
              (= (:tag ast) (:tag expr)) (contains? expr :info)
              (identical? (:form expr) (:val expr))
              (identical? (if (seq? (:form ast)) (second (:form ast)) nil)
                          (:val expr))
              (declaration/data (:purpose (meta (:val expr))))]]]
    (spit "out/quote-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
