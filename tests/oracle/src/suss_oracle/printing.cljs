(ns suss-oracle.printing
  (:require [suss-oracle.main :as transport]
            [suss-oracle.printing-cases :as cases]))

;; Development-only oracle: execute pinned upstream printing, not the adaptation.
(defn -main []
  (println (.stringify js/JSON
             #js {:schema 1
                  :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
                  :cases (into-array (cases/observations transport/encode))})))
(set! *main-cli-fn* -main)
