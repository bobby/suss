(ns suss-oracle.sorted-interfaces
  (:require [suss-oracle.main :as transport]
            [suss-oracle.sorted-interface-cases :as cases]))

;; Original development-only nominal probe; not a tree or record implementation.
(defn -main []
  (println (.stringify js/JSON
             #js {:schema 1
                  :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
                  :cases (into-array (cases/observations transport/encode))})))
(set! *main-cli-fn* -main)
