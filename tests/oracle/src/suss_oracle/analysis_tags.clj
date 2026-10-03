(ns suss-oracle.analysis-tags
  (:require [cljs.analyzer :as ana]
            [suss-oracle.declaration-macros :as declaration]))

;; Original development-only helper: inspect real analyzed AST fields and the
;; analyzer's public inference result, then emit the original expression once.
(defmacro observe [label expression]
  (when-not (string? label)
    (throw (ex-info "Expected an inference observation label" {:form &form})))
  (let [ast (ana/analyze &env expression)
        selected [:op :tag :ret-tag :inferred-ret-tag :children]
        row [label
             (mapv (fn [key] [(name key) (contains? ast key)
                             (declaration/data (get ast key))]) selected)
             (declaration/data (ana/get-tag ast))
             (declaration/data (ana/infer-tag &env ast))]]
    (spit "out/analysis-tag-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    expression))
