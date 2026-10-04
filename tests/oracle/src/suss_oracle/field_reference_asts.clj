(ns suss-oracle.field-reference-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original development projection. Read the actual analyzed initializer and
;; lexical field record; never reanalyze or execute a candidate expression.
(defmacro observe [label binding field]
  (let [ast (get-in &env [:locals binding :init])
        info (:info ast)
        lexical (get-in &env [:locals field])
        fields [:op :local :tag :children :val]
        info-fields [:local :field :mutable :unsynchronized-mutable
                     :volatile-mutable :tag :shadow :line :column]
        row [label
             (mapv (fn [key] [(name key) (contains? ast key)
                              (declaration/data (get ast key))]) fields)
             (mapv (fn [key] [(name key) (contains? info key)
                              (declaration/data (get info key))]) info-fields)
             [(= info lexical) (identical? info lexical)
              (= (:name ast) (:name info))
              (contains? info :init) (contains? ast :init)]]]
    (spit "out/field-reference-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
