(ns suss-oracle.method-role-facts
  (:require-macros [suss-oracle.method-role-macros :refer [fact]]))
(defprotocol Probe (probe [this x]))
(deftype Holder [^:mutable x]
  Probe (probe [this x] (fact "protocol-parameter"))
  Object (method [this x] (fact "object-parameter")))
(deftype FieldHolder [^:mutable x]
  Probe (probe [this arg]
    (fact "protocol-field")
    ((fn [this] (fact "nested-this")) this)
    (fact "protocol-restored"))
  Object (method [this arg] (fact "object-field")))
(defn -main [] (println (.stringify js/JSON (clj->js [(probe (Holder. 1) 2) (.method (Holder. 1) 2) (probe (FieldHolder. 1) 2) (.method (FieldHolder. 1) 2)]))))
(set! *main-cli-fn* -main)
