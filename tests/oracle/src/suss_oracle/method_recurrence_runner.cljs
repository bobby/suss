(ns suss-oracle.method-recurrence-runner
  (:require-macros [suss-oracle.method-recurrence :refer [observe]]))

(def results
  [(let [copy (fn* [x] x)] [(observe "plain" copy) (copy 42)])
   (let [copy (fn* [x] (if x (recur nil) 42))]
     [(observe "method-recur" copy) (copy 1)])
   (let [copy (fn* [x] (loop [y x] (if y (recur nil) 42)))]
     [(observe "nested-loop" copy) (copy 1)])
   (let [copy (fn* [x] (fn* [y] (if y (recur nil) x)))]
     [(observe "nested-function" copy) ((copy 42) 1)])
   (let [copy (fn* ([x] (if x (recur nil) 42)) ([x y] y))]
     [(observe "multiple-methods" copy) (copy 1 42)])
   (let [copy (fn* [x & xs] (if x (recur nil xs) 42))]
     [(observe "variadic" copy) (copy 1 2)])
   (let [copy (fn* [x] (if false (recur x) 42))]
     [(observe "unselected-recur" copy) (copy 1)])])
(defn -main []
  (println (.stringify js/JSON (clj->js results))))
(set! *main-cli-fn* -main)
