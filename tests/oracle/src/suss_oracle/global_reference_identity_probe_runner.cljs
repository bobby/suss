(ns suss-oracle.global-reference-identity-probe-runner
  (:require-macros [suss-oracle.global-reference-identity-probe :refer [observe]]))

(def ^{:doc "before"} watched 17)
(def initial
  [(let [copy watched] (observe "first" copy))
   (let [copy watched] (observe "same-revision" copy))])
(def ^{:doc "after"} watched 17)
(def changed
  [(let [copy watched] (observe "new-revision" copy))
   (let [copy watched] (observe "same-new-revision" copy))])

(defn -main []
  (println (.stringify js/JSON (clj->js [initial changed watched]))))
(set! *main-cli-fn* -main)
