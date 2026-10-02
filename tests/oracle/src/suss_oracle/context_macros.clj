(ns suss-oracle.context-macros)
(spit "out/context-facts-calls.jsonl" "")
(defmacro context [label]
  (spit "out/context-facts-calls.jsonl"
        (str "[" (pr-str label) "," (pr-str (name (:context &env))) "]\n") :append true)
  (name (:context &env)))
