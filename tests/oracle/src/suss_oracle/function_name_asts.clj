(ns suss-oracle.function-name-asts
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original projection of existing analyzer facts; never reanalyze a function.
(defmacro observe [label binding]
  (let [ast (get-in &env [:locals binding :init])
        local (:local ast)
        info (:info local)
        row [label [(contains? ast :name) (nil? (:name ast))
                    (contains? ast :local) (identical? (:name ast) local)
                    (mapv name (:children ast))
                    (when (:op local) (name (:op local)))
                    (when (:form local) (name (:form local)))
                    (when (:name local) (name (:name local)))
                    (when (:local local) (name (:local local)))
                    (contains? info :fn-self-name) (:fn-self-name info)
                    (contains? info :ns) (when (:ns info) (name (:ns info)))
                    (contains? info :shadow) (nil? (:shadow info))
                    (when (:name (:shadow info)) (name (:name (:shadow info))))
                    (contains? local :ret-tag) (if (symbol? (:ret-tag local)) (name (:ret-tag local)) (:ret-tag local))
                    (contains? info :fn-scope)
                    (mapv #(name (:name %)) (:fn-scope info))
                    (identical? local (last (:fn-scope (:env (first (:methods ast))))))]]]
    (spit "out/function-name-ast-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
