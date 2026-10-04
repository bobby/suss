(ns suss-oracle.local-reference-ast-runner
  (:require-macros [suss-oracle.local-reference-asts :refer [observe]]))

(def results
  [(let [x 1 copy x] (observe "let" copy))
   (let [x 1] (let [x false copy x] (observe "shadow" copy)))
   (loop [x 1] (let [copy x] (observe "loop" copy)))
   ((fn [x] (let [copy x] (observe "arg" copy))) 42)
   ((fn [x & more] (let [copy more] (observe "rest" copy))) 1 2)
   ((fn self [x] (let [copy self] (observe "fn" copy))) 1)])

(defn -main []
  (println (.stringify js/JSON (clj->js results))))
(set! *main-cli-fn* -main)
