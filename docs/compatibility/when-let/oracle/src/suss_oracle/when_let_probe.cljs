(ns suss-oracle.when-let-probe)
;; no Suss equality/printer calls.
(defn f64-bits [value]
  (let [view (js/DataView. (js/ArrayBuffer. 8))]
    (.setFloat64 view 0 value false)
    (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0")
         (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))))
(declare encode)
(defn items [value] (into-array (map encode value)))
(defn encode [value]
  (cond
    (instance? cljs.core/ExceptionInfo value) #js {:tag "exception-info" :data (encode (ex-data value)) :message (encode (ex-message value)) :cause (encode (ex-cause value))}
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

(def result (do

          (def when-let-events (atom []))
          [(when-let [x nil] x)
           (when-let [x false] x)
           (when-let [x 0] x)
           (when-let [x ""] x)
           (when-let [x []] x)
           (when-let [x {}] x)
           (when-let [x true])
           (when-let [x nil])
           (when-let [x (do (swap! when-let-events conj 1) 17)]
             (swap! when-let-events conj 10)
             (swap! when-let-events conj 20)
             x)
           (when-let [x (do (swap! when-let-events conj 2) false)]
             (swap! when-let-events conj 999))
           (let [x 31] (when-let [x (+ x 1)] x))
           (let [temp 77] (when-let [x 17] [temp x]))
           (when-let [[a b] [17 23]] [a b])
           (when-let [{:keys [a b] :or {b 23}} {:a 17}] [a b])
           (loop [n 3] (if (= n 0) 7 (when-let [x n] (recur (- x 1)))))
           @when-let-events]
        
))
(defn -main [] (println (.stringify js/JSON #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1" :value (encode result)})))
(set! *main-cli-fn* -main)
