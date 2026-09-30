(ns suss-oracle.implements
  (:require [suss-oracle.main :as transport]
            [suss-oracle.implements-cases :as cases]
            [suss-oracle.implementation-referrer :as referrer]))

;; Original development-only probe; no runtime implementation is copied.
(defn -main []
  (when-not (referrer/verify-refers)
    (throw (js/Error. "Explicit user implements? refer/alias differs")))
  (println (.stringify js/JSON
             #js {:schema 1
                  :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
                  :cases (into-array (cases/observations transport/encode))})))
(set! *main-cli-fn* -main)
