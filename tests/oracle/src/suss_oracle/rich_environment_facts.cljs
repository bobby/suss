(ns suss-oracle.rich-environment-facts
  (:require-macros [suss-oracle.rich-environment-macros :refer [environment]]))
(environment "top")
(def literal (let [^:private x 41] (environment "literal")))
(def shadowed (let [x 40] (let [x (+ x 2)] (environment "shadow"))))
(def invoked (let [x (identity 42)] (environment "invoke")))
(def branched (let [x (if true 41 42)] (environment "if")))
(def repeated (loop [x 0] (environment "loop")))
(def caught (try (throw 42) (catch :default problem (environment "catch"))))
(def named (fn self [^number x & xs] (environment "function")))
(def nested (fn outer [x] ((fn inner [x] (environment "nested")) x)))
(def anonymous (fn [x] (environment "anonymous-hint")))
(defprotocol Probe (probe [receiver value]))
(deftype Holder [^:mutable x]
  Probe (probe [receiver x] (environment "protocol-parameter"))
  Object (method [receiver x] (environment "object-parameter")))
(deftype FieldHolder [^:mutable x]
  Probe (probe [receiver arg]
    (environment "protocol-field")
    ((fn [receiver] (environment "nested-receiver")) receiver)
    (environment "protocol-restored"))
  Object (method [receiver arg] (environment "object-field")))
(defn -main []
  (println (.stringify js/JSON
    (clj->js [literal shadowed invoked branched repeated caught
              (named 1 2) (nested 1) (anonymous 1)
              (probe (Holder. 1) 2) (.method (Holder. 1) 2)
              (probe (FieldHolder. 1) 2) (.method (FieldHolder. 1) 2)]))))
(set! *main-cli-fn* -main)
