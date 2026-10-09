(ns suss-oracle.object-length-conversion)
;; Original ArraySetLength effect reference forms.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "length-two-conversions-mismatch" :value (let [n (atom 0) a (array 7 8 9) rhs (js-obj "valueOf" (fn [] (swap! n inc)))] (try (aset a "length" rhs) false (catch :default e (and (= (.-message e) "Invalid array length") (= @n 2) (= (alength a) 3)))))}
#js {:id "length-second-conversion-throw" :value (let [n (atom 0) a (array 7 8 9) rhs (js-obj "valueOf" (fn [] (if (= (swap! n inc) 1) 1 (throw 17))))] (try (aset a "length" rhs) false (catch :default e (and (= e 17) (= @n 2) (= (alength a) 3)))))}
#js {:id "length-first-conversion-mutation" :value (let [n (atom 0) a (array 7 8 9) rhs (js-obj "valueOf" (fn [] (swap! n inc) (if (= @n 1) (do (.push a 23) 1) 1)))] (aset a "length" rhs) (and (= @n 2) (= (alength a) 1) (= (aget a 0) 7) (not (.hasOwnProperty a "3"))))}
#js {:id "length-first-conversion-uint32-modulo" :value (let [n (atom 0) a (array 7 8 9) rhs (js-obj "valueOf" (fn [] (if (= (swap! n inc) 1) 4294967297 1)))] (aset a "length" rhs) (and (= @n 2) (= (alength a) 1)))}
]))))
(set! *main-cli-fn* -main)
