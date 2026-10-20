(ns suss-oracle.short-property)
(deftype ShortLiteralFields [ns name])
(defn bits [value] (let [view (js/DataView. (js/ArrayBuffer. 8))] (.setFloat64 view 0 value false) (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0") (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))))
(defn encode [value] (cond (nil? value) #js {:tag "nil"} (boolean? value) #js {:tag "bool" :value value} (number? value) #js {:tag "f64" :bits (bits value)} (vector? value) #js {:tag "vector" :items (into-array (map encode value))} :else (throw (ex-info "unsupported value" {}))))
(defn -main [] (println (.stringify js/JSON #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1" :cases (into-array [
#js {:id "nominal-nil-false-value" :value (encode (let [read (fn [o] (.-ns o))] [(read (ShortLiteralFields. nil 0)) (read (ShortLiteralFields. false 0)) (read (ShortLiteralFields. 23 0))]))}
#js {:id "replaced-getter-ordered-owner-effects" :value (encode (let [events (atom []) o (js-obj) read (fn [v] (.-ns v))] (.__defineGetter__ o "ns" (fn [] (swap! events conj 2) 17)) (let [a (read (do (swap! events conj 1) o))] (.__defineGetter__ o "ns" (fn [] (swap! events conj 3) 19)) (let [b (read (do (swap! events conj 1) o))] [a b @events]))))}
#js {:id "fallback-nil-false-getters-run-once" :value (encode (let [events (atom []) o (js-obj) read (fn [v] (.-ns v))] (.__defineGetter__ o "ns" (fn [] (swap! events conj 1) nil)) (let [a (read o)] (.__defineGetter__ o "ns" (fn [] (swap! events conj 2) false)) [a (read o) @events])))}
#js {:id "fallback-thrown-nil-false-preserve-effects" :value (encode (let [events (atom []) o (js-obj) read (fn [v] (.-ns v))] (.__defineGetter__ o "ns" (fn [] (swap! events conj 2) (throw nil))) (let [a (try (read (do (swap! events conj 1) o)) [false 0] (catch :default value [true value]))] (.__defineGetter__ o "ns" (fn [] (swap! events conj 4) (throw false))) (let [b (try (read (do (swap! events conj 3) o)) [false 0] (catch :default value [true value]))] [a b @events]))))}
])})))
(set! *main-cli-fn* -main)
