(ns suss-oracle.lazy-sequences
  (:require [suss-oracle.lazy-sequence-cases :as cases]))

;; Fresh actual pinned execution; scalar arrays preserve the observation order.
(defn -main []
  (println (.stringify js/JSON
    #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :cases #js [#js ["realize" (into-array (cases/lazy-realize-observations))]
                    #js ["metadata-retry" (into-array (cases/lazy-meta-retry-observations))]
                    #js ["concat" (into-array (cases/concat-observations))]
                    #js ["constructors" (into-array (cases/reader-constructor-observations))]]})))
(set! *main-cli-fn* -main)
