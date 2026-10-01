(ns suss-oracle.bitwise-capture
  (:require [suss-oracle.main :as transport]
            [suss-oracle.bitwise-capture-cases :as cases]))

;; Development-only diagnostics; internal-body js* does not advertise host interop.
(defn -main []
  (println (.stringify js/JSON
    #js {:schema 1
         :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :cases (into-array (cases/observations transport/encode))})))
(set! *main-cli-fn* -main)
