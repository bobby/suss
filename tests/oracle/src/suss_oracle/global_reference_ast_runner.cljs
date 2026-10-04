(ns suss-oracle.global-reference-ast-runner
  (:require-macros [suss-oracle.global-reference-asts :refer [observe]]))

(def effects 0)
(def ^{:doc "source declaration"} scalar
  (do (set! effects (+ effects 1)) 42))
(def ^string hinted 1)
(def ^:dynamic *dynamic* 7)
(def fixed (fn [] 42))
(declare pending)
(def ^{:name forged :ns wrong :doc "raw metadata" :tag number} overridden 3)

(def results
  [(let [copy scalar] (observe "scalar" copy scalar))
   (let [copy suss-oracle.global-reference-ast-runner/scalar] (observe "qualified" copy scalar))
   (let [copy hinted] (observe "hint" copy hinted))
   (let [copy *dynamic*] (observe "dynamic" copy *dynamic*))
   (let [copy fixed] (observe "function" copy fixed))
   (let [copy pending] (observe "declared" copy pending))
   (let [copy overridden] (observe "raw-metadata" copy overridden))
   (let [copy identity] (observe "core" copy identity))
   (let [copy cljs.core/identity] (observe "core-alias" copy identity))
   (let [copy scalar
         changed (def ^{:doc "changed declaration"} scalar false)]
     (observe "revision-before" copy scalar))
   (let [copy scalar] (observe "revision-after" copy scalar))])

(defn -main []
  (println (.stringify js/JSON (clj->js [results scalar effects]))))
(set! *main-cli-fn* -main)
