(ns suss-oracle.direct-let-probe)
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


(def result1 (do
          (def let-pattern-events (atom []))
          (let [outer 23
                __suss_destructure_vec_0 77
                [a b :as whole] (do (swap! let-pattern-events conj 1) [17 19])
                {:keys [x y] :or {x (do (swap! let-pattern-events conj 2) outer)
                                      y (do (swap! let-pattern-events conj 3) a)}}
                  (do (swap! let-pattern-events conj 4) {:y b})
                later (do (swap! let-pattern-events conj 5) (+ a x y))]
            [a b whole x y later __suss_destructure_vec_0 @let-pattern-events])
        
))
(def result2 (do
          [(let [x 11] (let [[x y] [x 7] z (+ x y)] [x y z]))
           (loop [n 3 total 0]
             (if (= n 0) total
               (let [[step] [n] {:keys [delta]} {:delta step}]
                 (recur (- n 1) (+ total delta)))))]
        
))
(defn -main [] (println (.stringify js/JSON #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1" :values #js [(encode result1) (encode result2)]})))
(set! *main-cli-fn* -main)
