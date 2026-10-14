(ns suss-oracle.realized
  (:require [suss-oracle.realized-cases :as cases]))

(defn -main []
  (println (.stringify js/JSON
    #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :cases #js [#js ["lazy" (into-array (cases/realized-lazy-observations))]
                    #js ["empty" (into-array (cases/realized-empty-observations))]
                    #js ["sequence" (into-array (cases/realized-sequence-observations))]
                    #js ["protocol" (into-array (cases/realized-protocol-observations))]]})))
(set! *main-cli-fn* -main)
