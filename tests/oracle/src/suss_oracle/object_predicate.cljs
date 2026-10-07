(ns suss-oracle.object-predicate
  (:require [suss-oracle.main :as transport]
            [suss-oracle.object-predicate-cases :as cases]))

;; Development-only pinned oracle; no target predicate or printer is called.
(defn -main []
  (println (.stringify js/JSON
             #js {:schema 1
                  :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
                  :cases (into-array (cases/observations transport/encode))})))
(set! *main-cli-fn* -main)
