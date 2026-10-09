(ns suss-oracle.array-make-dimensions)
;; Original reference forms shared with native execution.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "negative-dynamic-outer" :value (let [n -1] (try (make-array nil n 2) false (catch :default e (= (.-message e) "Invalid array length"))))}
#js {:id "fractional-dynamic-outer" :value (let [n 1.5] (try (make-array nil n 2) false (catch :default e (= (.-message e) "Invalid array length"))))}
#js {:id "negative-leaf" :value (try (make-array nil 2 -1) false (catch :default e (= (.-message e) "Invalid array length")))}
#js {:id "zero-outer-invalid-leaf" :value (= (alength (make-array nil 0 -1)) 0)}
#js {:id "zero-outer-invalid-middle" :value (= (alength (make-array nil 0 -1 2)) 0)}
#js {:id "nonnumeric-outer" :value (let [a (make-array nil "2" 3)] (and (= (alength a) 1) (= (alength (aget a 0)) 3) (not (.hasOwnProperty (aget a 0) "0"))))}
#js {:id "nonnumeric-leaf" :value (let [a (make-array nil 2 "3")] (and (= (alength a) 2) (= (alength (aget a 0)) 1) (= (aget a 0 0) "3")))}
#js {:id "negative-literal-outer-empty" :value (= (alength (make-array nil -1 2)) 0)}
#js {:id "fractional-literal-outer-ceil" :value (let [a (make-array nil 1.5 2)] (and (= (alength a) 2) (= (alength (aget a 0)) 2)))}
]))))
(set! *main-cli-fn* -main)
