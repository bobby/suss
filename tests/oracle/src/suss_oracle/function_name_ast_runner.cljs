(ns suss-oracle.function-name-ast-runner
  (:require-macros [suss-oracle.function-name-asts :refer [observe]]))

(def results
  [(let [copy (fn* [x] x)] [(observe "anonymous" copy) (copy 42)])
   (let [copy (fn* n [x] x)] [(observe "named" copy) (copy 42)])
   (let [n 7 copy (fn* n [x] x)] [(observe "shadowed" copy) (copy 42)])
   (let [copy (fn* ^number n [x] x)] [(observe "tagged" copy) (copy 42)])
   (let [copy (fn* ^{:tag false} n [x] x)] [(observe "false-tag" copy) (copy 42)])
   (let [copy (fn* ^{:tag nil} n [x] x)] [(observe "nil-tag" copy) (copy 42)])
   (let [copy (fn* n ([x] x) ([x y] y))] [(observe "multiple-methods" copy) (copy 1 42)])])
(defn -main []
  (println (.stringify js/JSON (clj->js results))))
(set! *main-cli-fn* -main)
