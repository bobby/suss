(ns suss-oracle.collection-source-ast-runner
  (:require-macros [suss-oracle.collection-source-asts :refer [observe]]))

(def effects 0)
(def results
  [(let [copy []] (observe "empty-vector" copy))
   (let [copy [nil false 42 :key "text"]] (observe "vector" copy))
   (let [copy {}] (observe "empty-map" copy))
   (let [copy {:a 1 :b false}] (observe "map" copy))
   (let [copy #{}] (observe "empty-set" copy))
   (let [copy #{1 false :key}] (observe "set" copy))
   (let [copy [{:a #{1 2}}]] (observe "nested" copy))
   (let [x false copy [x]] (observe "local-child" copy))
   (let [copy ['unresolved]] (observe "quoted-child" copy))
   (let [copy ^{:purpose :kept} [1]] (observe "metadata-vector" copy))
   (let [copy [(do (set! effects (+ effects 1)) 42)]]
     (observe "effect-child" copy))
   (let [copy [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32]]
     (observe "factory-vector" copy))
   (let [copy {:k0 0 :k1 1 :k2 2 :k3 3 :k4 4 :k5 5 :k6 6 :k7 7 :k8 8}]
     (observe "factory-map" copy))
   (let [copy #{0 1 2 3 4 5 6 7 8}]
     (observe "factory-set" copy))])
(defn -main []
  (println (.stringify js/JSON (clj->js [results effects]))))
(set! *main-cli-fn* -main)
