(ns suss-oracle.array-push-properties)
;; Original reference fixture; evaluated with the pinned upstream compiler.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "push-max-three-writes" :value (let [ctor (type (array)) a (ctor 4294967295) trace (atom [])] (try (.push a (do (swap! trace conj :a) 17) (do (swap! trace conj :b) 18) (do (swap! trace conj :c) 19)) false (catch :default e (and (= (.-message e) "Invalid array length") (= @trace [:a :b :c]) (= (alength a) 4294967295) (= (aget a 4294967295) 17) (= (aget a 4294967296) 18) (= (aget a 4294967297) 19) (.hasOwnProperty a "4294967297")))))}
#js {:id "push-max-minus-one-three-writes" :value (let [ctor (type (array)) a (ctor 4294967294)] (try (.push a 17 18 19) false (catch :default e (and (= (.-message e) "Invalid array length") (= (alength a) 4294967295) (= (aget a 4294967294) 17) (= (aget a 4294967295) 18) (= (aget a 4294967296) 19) (.hasOwnProperty a "4294967296")))))}
#js {:id "push-to-max-succeeds" :value (let [ctor (type (array)) a (ctor 4294967294)] (and (= (.push a 23) 4294967295) (= (alength a) 4294967295) (= (aget a 4294967294) 23)))}
#js {:id "push-max-empty-succeeds" :value (let [ctor (type (array)) a (ctor 4294967295)] (and (= (.push a) 4294967295) (= (alength a) 4294967295)))}
#js {:id "ordinary-numeric-properties" :value (let [a (array)] (aset a -1 false) (aset a 1.5 nil) (aset a ##NaN 17) (aset a ##Inf 19) (and (= (alength a) 0) (false? (aget a "-1")) (nil? (aget a "1.5")) (= (aget a "NaN") 17) (= (aget a "Infinity") 19) (.hasOwnProperty a "1.5")))}
#js {:id "ordinary-string-properties-and-index-spelling" :value (let [a (array)] (aset a "01" 17) (aset a "0" 19) (aset a "4294967296" 23) (and (= (alength a) 1) (= (aget a 0) 19) (= (aget a "01") 17) (= (aget a 4294967296) 23) (.hasOwnProperty a "01") (.hasOwnProperty a 4294967296)))}
#js {:id "indexed-length-shrink-preserves-ordinary" :value (let [a (array 17 19)] (aset a "label" 23) (aset a "length" 1) (aset a "length" 3) (and (= (aget a "length") 3) (= (alength a) 3) (= (aget a 0) 17) (undefined? (aget a 1)) (not (.hasOwnProperty a "1")) (= (aget a "label") 23)))}
#js {:id "invalid-length-assignment-no-write" :value (let [a (array 17)] (try (aset a "length" 1.5) false (catch :default e (and (= (.-message e) "Invalid array length") (= (alength a) 1) (= (aget a "length") 1) (= (aget a 0) 17)))))}
]))))
(set! *main-cli-fn* -main)
