(ns suss-oracle.field-reference-ast-runner
  (:require-macros [suss-oracle.field-reference-asts :refer [observe]]))

(defprotocol Probe (probe [this]))
(deftype FieldAstPlain [x]
  Probe (probe [this] (let [copy x] (observe "plain" copy x))))
(deftype FieldAstMutable [^:mutable x]
  Probe (probe [this] (let [copy x] (observe "mutable" copy x))))
(deftype FieldAstUnsynchronized [^:unsynchronized-mutable x]
  Probe (probe [this] (let [copy x] (observe "unsynchronized" copy x))))
(deftype FieldAstVolatile [^:volatile-mutable x]
  Probe (probe [this] (let [copy x] (observe "volatile" copy x))))
(deftype FieldAstFalseMutable [^{:mutable false} x]
  Probe (probe [this] (let [copy x] (observe "false-mutable" copy x))))
(deftype FieldAstNumberHint [^number x]
  Probe (probe [this] (let [copy x] (observe "number-hint" copy x))))
(deftype FieldAstNilHint [^{:tag nil} x]
  Probe (probe [this] (let [copy x] (observe "nil-hint" copy x))))
(deftype FieldAstFalseHint [^{:tag false} x]
  Probe (probe [this] (let [copy x] (observe "false-hint" copy x))))
(deftype FieldAstShadowed [x]
  Probe (probe [this]
    (let [x 7 copy x] (observe "shadow" copy x))))

(def results [(probe (FieldAstPlain. 42)) (probe (FieldAstMutable. 42))
              (probe (FieldAstUnsynchronized. 42)) (probe (FieldAstVolatile. 42))
              (probe (FieldAstFalseMutable. 42)) (probe (FieldAstNumberHint. 42))
              (probe (FieldAstNilHint. 42)) (probe (FieldAstFalseHint. 42))
              (probe (FieldAstShadowed. 42))])
(defn -main [] (println (.stringify js/JSON (clj->js results))))
(set! *main-cli-fn* -main)
