(ns suss-oracle.collection-source-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original development-only projection of already analyzed source nodes.
;; Preserve edge presence and original ordering. Never analyze a node again.
(defn project [ast depth]
  (when (> depth 8)
    (throw (ex-info "Collection AST projection depth exceeded" {})))
  [(mapv (fn [key] [(name key) (contains? ast key)
                     (declaration/data (get ast key))])
          [:op :tag :form :children :literal?])
   [(contains? ast :val) (= :const (:op ast))
    (when (= :const (:op ast)) (declaration/data (:val ast)))]
   ;; Follow genuine analyzer child edges. :val is a child AST for set!,
   ;; unlike literal const data; never serialize an entire env/info map.
   (mapv (fn [key]
           (let [value (get ast key)]
             [(name key) (contains? ast key)
              (if (vector? value)
                ["many" (mapv #(project % (inc depth)) value)]
                ["one" (project value (inc depth))])]))
         (:children ast))])

(defmacro observe [label binding]
  (let [row [label (project (get-in &env [:locals binding :init]) 0)]]
    (spit "out/collection-source-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
