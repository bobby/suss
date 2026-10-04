(ns suss-oracle.local-reference-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original development projection of the initializer already analyzed by the
;; pinned compiler. Do not analyze the expression a second time or execute it.
(defmacro observe [label binding]
  (let [ast (get-in &env [:locals binding :init])
        info (:info ast)
        fields [:op :local :arg-id :variadic?]
        row [label
             (mapv (fn [key] [(name key) (contains? ast key)
                              (declaration/data (get ast key))]) fields)
             [(= info (get-in &env [:locals (:form ast)]))
              (= (:name ast) (:name info))
              (= (contains? ast :init) (contains? info :init))
              (= (:init ast) (:init info))
              (contains? ast :val) (contains? ast :children)
              (identical? info (get-in &env [:locals (:form ast)]))
              (identical? (:init ast) (:init info))]]]
    (spit "out/local-reference-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
