(ns suss-oracle.function-scope-macros
  (:require [clojure.string :as str]))
(spit "out/function-scope-calls.jsonl" "")
(defn json-strings [values] (str "[" (str/join "," (map pr-str values)) "]"))
(defmacro scope [label]
  (let [names (mapv #(str (:name %)) (:fn-scope &env))
        locals (sort (map str (keys (:locals &env))))]
    (spit "out/function-scope-calls.jsonl"
          (str "{\"label\":" (pr-str label) ",\"names\":" (json-strings names)
               ",\"locals\":" (json-strings locals) "}\n") :append true)
    names))
