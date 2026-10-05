(ns suss-oracle.control-source-ast-runner
  (:require-macros [suss-oracle.control-source-asts :refer [observe]]))

(def effects 0)
(def results
  [(let [copy (do)] (observe "empty-do" copy))
   (let [copy (do 7)] (observe "single-do" copy))
   (let [copy (do 1 2)] (observe "multi-do" copy))
   (let [copy (if true 1 2)] (observe "if-true" copy))
   (let [copy (if false 1 2)] (observe "if-false" copy))
   (let [copy (if false 1)] (observe "if-implicit-nil" copy))
   (let [copy (let [x 1] x)] (observe "let-local" copy))
   (let [copy (let [x 1 y x] y)] (observe "let-sequential" copy))
   (let [copy (let* [x 1] x)] (observe "special-let" copy))
   (let [copy (loop [x 1] x)] (observe "loop-local" copy))
   (let [copy (loop [x 1] (if false (recur 2) x))]
     (observe "loop-recur" copy))
   (let [copy (fn* [x] x)] (observe "fixed-function" copy))
   (let [copy (fn* [x & rest] rest)] (observe "variadic-function" copy))
   (let [copy (try (throw 42) (catch :default error error))]
     (observe "try-catch" copy))
   (let [copy (try (do (set! effects 1) 7) (finally (set! effects 2)))]
     (observe "try-finally" copy))])
(defn -main []
  (println (.stringify js/JSON (clj->js [results effects]))))
(set! *main-cli-fn* -main)
