(ns suss-oracle.method-role-macros
  (:require [clojure.string :as str]))
(spit "out/method-role-calls.jsonl" "")
(defn json-value [value]
  (cond
    (nil? value) "null"
    (string? value) (pr-str value)
    (keyword? value) (pr-str (name value))
    (boolean? value) (str value)
    (and (integer? value) (<= 0 value)) (str value)
    (vector? value) (str "[" (str/join "," (map json-value value)) "]")
    :else (throw (ex-info "Unexpected compiler role observation" {:value value}))))
(defn role [binding]
  (when binding [(:local binding) (:arg-id binding) (boolean (:mutable binding))]))
(defmacro fact [label]
  (let [bindings (mapv (fn [[name binding]]
                    [(str name) (role binding) (role (:shadow binding))])
                  (sort-by (comp str key) (:locals &env)))]
    (spit "out/method-role-calls.jsonl" (str (json-value [label bindings]) "\n") :append true)
    (count (:locals &env))))
