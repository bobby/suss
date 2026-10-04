(ns suss-oracle.quote-ast-runner
  (:require-macros [suss-oracle.quote-asts :refer [observe]]))

(def effects 0)
(def results
  [
   (let [copy (quote nil)] (observe "nil" copy))
   (let [copy (quote false)] (observe "boolean" copy))
   (let [copy (quote 42)] (observe "number" copy))
   (let [copy (quote "hello")] (observe "string" copy))
   (let [copy (quote :word)] (observe "keyword" copy))
   (let [copy (quote unresolved)] (observe "symbol" copy))
   (let [copy (quote missing.ns/name)] (observe "qualified" copy))
   (let [copy (quote ())] (observe "empty-list" copy))
   (let [copy (quote (unresolved 42))] (observe "list" copy))
   (let [copy (quote [unresolved 42])] (observe "vector" copy))
   (let [copy (quote {:a unresolved :b 42})] (observe "map" copy))
   (let [copy (quote #{:a :b})] (observe "set" copy))
   (let [copy (quote ^{:purpose :kept} unresolved)] (observe "metadata" copy))
   (let [copy (quote (do (set! effects (+ effects 1)) 42))] (observe "effect-data" copy))])

(defn -main []
  (println (.stringify js/JSON (clj->js [results effects]))))
(set! *main-cli-fn* -main)
