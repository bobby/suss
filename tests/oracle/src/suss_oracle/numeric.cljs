(ns suss-oracle.numeric
  (:require [suss-oracle.main :as transport]
            [suss-oracle.numeric-cases :as cases]))

;; Original development oracle. Inputs are lossless bits, not rounded JSON floats.
(defn observe [bits]
  (let [view (js/DataView. (js/ArrayBuffer. 8))]
    (.setUint32 view 0 (js/parseInt (subs bits 0 8) 16) false)
    (.setUint32 view 4 (js/parseInt (subs bits 8) 16) false)
    (let [number (.getFloat64 view 0 false)
          text (+ number "")]
      #js {:id bits :bits bits
           :formatted (transport/encode text)
           :parsed (transport/encode (* text 1))})))
(defn -main []
  (println (.stringify js/JSON
    #js {:schema 1
         :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
         :cases (into-array (map observe cases/bits))})))
(set! *main-cli-fn* -main)
