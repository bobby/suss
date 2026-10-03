(ns suss-oracle.syntax-quote
  (:require [suss-oracle.syntax-quote-cases :as cases]))

;; Actual pinned compiled source and Node execution, not a reader-only probe.
(defn -main []
  (println (.stringify js/JSON
    #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :cases #js [#js ["execution" (into-array (cases/syntax-quote-observations))]]})))
(set! *main-cli-fn* -main)
