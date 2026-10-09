(ns suss-oracle.array-public-properties)
;; Original reference forms shared with executing native regressions.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "named-length" :value (let [a (array 17 19)] (set! (.-length a) 1) (and (= (.-length a) 1) (= (aget a "length") 1) (undefined? (aget a 1))))}
#js {:id "string-length-rhs" :value (let [a (array) rhs "2"] (and (identical? (aset a "length" rhs) rhs) (= (alength a) 2) (not (.hasOwnProperty a "0"))))}
#js {:id "boolean-true-length" :value (let [a (array 17 19)] (aset a "length" true) (and (= (alength a) 1) (= (aget a 0) 17)))}
#js {:id "boolean-false-length" :value (let [a (array 17)] (aset a "length" false) (and (= (alength a) 0) (not (.hasOwnProperty a "0"))))}
#js {:id "nil-length" :value (let [a (array 17)] (aset a "length" nil) (= (alength a) 0))}
#js {:id "shadow-push" :value (let [a (array) f (fn [] 23)] (aset a "push" f) (and (identical? (.-push a) f) (= (.push a) 23) (= (alength a) 0) (.hasOwnProperty a "push")))}
#js {:id "dynamic-make-holes" :value (let [n 2 a (make-array n)] (and (= (alength a) 2) (undefined? (aget a 0)) (not (.hasOwnProperty a "0"))))}
#js {:id "first-class-make-holes" :value (let [f make-array a (f 2)] (and (= (alength a) 2) (not (.hasOwnProperty a "1"))))}
#js {:id "literal-make-nil-own" :value (let [a (make-array 2)] (and (nil? (aget a 0)) (.hasOwnProperty a "0")))}
#js {:id "multi-make-leaf-holes" :value (let [a (make-array nil 2 3)] (and (.hasOwnProperty a "0") (= (alength (aget a 0)) 3) (not (.hasOwnProperty (aget a 0) "0")) (not (identical? (aget a 0) (aget a 1)))))}
#js {:id "dynamic-make-max-holes" :value (let [n 4294967295 a (make-array n)] (and (= (alength a) n) (not (.hasOwnProperty a "4294967294"))))}
]))))
(set! *main-cli-fn* -main)
