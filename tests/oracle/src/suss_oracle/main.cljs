(ns suss-oracle.main
  (:require [suss-oracle.cases]))

;; Original development oracle transport; no Suss equality/printer calls.
(defn f64-bits [value]
  (let [view (js/DataView. (js/ArrayBuffer. 8))]
    (.setFloat64 view 0 value false)
    (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0")
         (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))))
(declare encode)
(defn items [value] (into-array (map encode value)))
(defn encode [value]
  (cond
    (nil? value) #js {:tag "nil"}
    (boolean? value) #js {:tag "bool" :value value}
    (number? value) #js {:tag "f64" :bits (f64-bits value)}
    (string? value) #js {:tag "string" :units (into-array (map #(.charCodeAt value %) (range (.-length value))))}
    (keyword? value) #js {:tag "keyword" :namespace (encode (namespace value)) :name (encode (name value))}
    (symbol? value) #js {:tag "symbol" :namespace (encode (namespace value)) :name (encode (name value))}
    (map? value) #js {:tag "map" :entries (into-array (map (fn [[k v]] #js [(encode k) (encode v)]) value))}
    (set? value) #js {:tag "set" :items (items value)}
    (vector? value) #js {:tag "vector" :items (items value)}
    (seq? value) #js {:tag "seq" :items (items value)}
    :else (throw (ex-info "unsupported oracle value" {:type (type value)}))))
(defn observation [identity thunk]
  (let [trace (atom [])]
    (try
      #js {:id identity :status "value" :value (encode (thunk trace)) :effects (items @trace)}
      (catch :default error
        #js {:id identity :status "exception" :data (encode (ex-data error))
             :message (encode (ex-message error)) :effects (items @trace)}))))
(defn -main []
  (let [cases (suss-oracle.cases/observations observation)]
    (println (.stringify js/JSON #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1" :cases (into-array cases)}))))
(set! *main-cli-fn* -main)
