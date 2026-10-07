(ns suss-oracle.constructor-source-asts
  (:require [suss-oracle.collection-source-asts :as source]
            [suss-oracle.declaration-macros :as declaration]))

;; Original development-only projection of already analyzed child edges.
(defmacro observe [label binding]
  (let [ast (get-in &env [:locals binding :init])
        info (get-in ast [:class :info])
        row [label (source/project ast 0)
             (mapv (fn [key] [(name key) (contains? info key)
                              (declaration/data (get info key))])
                   [:type :num-fields :record :private])]]
    (spit "out/constructor-source-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
