(ns suss-oracle.source-environment-macros
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original source macro fixtures. Capture the actual implicit arguments and
;; return quoted facts so inspected initializer syntax is never executed again.
(defn record [label facts]
  (spit "out/source-environment-calls.jsonl"
        (str (declaration/json [label (declaration/data facts)]) "\n") :append true)
  (list 'quote facts))

(defmacro lexical-facts [name]
  (let [binding (get (:locals &env) name)]
    (record "lexical"
            [(:op binding) (:local binding) (:form binding)
             (get-in binding [:init :form]) (get-in binding [:shadow :local])
             (:context &env) (nth &form 1)])))

(defmacro scope-facts
  ([] (record "function"
              [(:context &env) (count (:fn-scope &env))
               (get-in &env [:locals 'x :local])]))
  ([name & more]
   (record "variadic"
           [(get-in &env [:ns :name]) (get-in &env [:locals name :local])
            (count more) (count &form)])))
