(ns suss-oracle.reader-boundary
  (:require [cljs.tools.reader :as reader]))

;; Original development observation adapter. No code is imported into Suss.
(defn encode [value]
  (cond
    (number? value)
    (let [view (js/DataView. (js/ArrayBuffer. 8))]
      (.setFloat64 view 0 value false)
      #js {:tag "f64"
           :bits (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0")
                      (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))})
    (string? value)
    #js {:tag "string" :units (into-array (map #(.charCodeAt value %) (range (.-length value))))}
    :else (throw (js/Error. "Unsupported reader boundary observation"))))

(defn -main []
  (let [corpus (js/JSON.parse (.readFileSync (js/require "fs") "reader-cases.json" "utf8"))]
    (println (.stringify js/JSON
      #js {:schema (.-schema corpus) :upstream (.-upstream corpus)
           :cases (into-array
                    (map (fn [entry]
                           #js {:id (.-id entry)
                                :value (encode (reader/read-string (.-source entry)))})
                         (array-seq (.-cases corpus))))}))))
(set! *main-cli-fn* -main)
