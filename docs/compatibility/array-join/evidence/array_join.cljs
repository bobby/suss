(ns suss-oracle.array-join)
;; Original Array join value/effect reference forms.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "scalars-and-empty-nullish" :value (= (.join (array 1 false nil (aget (array) 0) "x")) "1,false,,,x")}
#js {:id "holes-and-separator" :value (let [ctor (type (array)) a (ctor 3)] (aset a 1 "x") (= (.join a "-") "-x-"))}
#js {:id "nested-arrays" :value (= (.join (array (array 1 2) (array 3))) "1,2,3")}
#js {:id "self-cycle" :value (let [a (array 1)] (aset a 1 a) (= (.join a) "1,"))}
#js {:id "mutual-cycle" :value (let [a (array "a") b (array "b")] (aset a 1 b) (aset b 1 a) (= (.join a) "a,b,"))}
#js {:id "live-later-write" :value (let [a (array nil 2) x (js-obj "toString" (fn [] (aset a 1 7) "x"))] (aset a 0 x) (= (.join a) "x,7"))}
#js {:id "separator-shrinks-after-length-capture" :value (let [a (array 1 2) sep (js-obj "toString" (fn [] (aset a "length" 1) "-"))] (= (.join a sep) "1-"))}
#js {:id "element-shrinks-after-length-capture" :value (let [a (array nil 2) x (js-obj "toString" (fn [] (aset a "length" 1) "x"))] (aset a 0 x) (= (.join a) "x,"))}
#js {:id "join-override" :value (let [a (array 1 2)] (aset a "join" (fn [] "overridden")) (= (.toString a) "overridden"))}
#js {:id "noncallable-join-fallback" :value (let [a (array 1)] (aset a "join" 7) (= (.toString a) "[object Array]"))}
#js {:id "number-constructor-array" :value (let [ctor (type 1)] (= (ctor (array 1)) 1))}
#js {:id "string-constructor-array" :value (let [ctor (type "x")] (= (ctor (array 1)) "1"))}
#js {:id "large-empty-sparse" :value (let [ctor (type (array)) a (ctor 1000001)] (= (.join a "") ""))}
#js {:id "large-last-sparse" :value (let [ctor (type (array)) a (ctor 1000001)] (aset a 1000000 "x") (= (.join a "") "x"))}
#js {:id "nested-exception-cleanup" :value (let [b (array (js-obj "toString" (fn [] (throw 17)))) a (array b)] (try (.join a) (catch :default e nil)) (aset b 0 "fixed") (= (.join a) "fixed"))}
#js {:id "live-sparse-insertion" :value (let [ctor (type (array)) a (ctor 1000001) x (js-obj "toString" (fn [] (aset a 1000000 "later") "first"))] (aset a 0 x) (= (.join a "") "firstlater"))}
#js {:id "live-sparse-shrink-removal" :value (let [ctor (type (array)) a (ctor 1000001) x (js-obj "toString" (fn [] (aset a "length" 1) "first"))] (aset a 0 x) (aset a 1000000 "removed") (= (.join a "") "first"))}
#js {:id "growth-outside-captured-length" :value (let [ctor (type (array)) a (ctor 2) x (js-obj "toString" (fn [] (aset a 1000000 "outside") "first"))] (aset a 0 x) (= (.join a "") "first"))}
]))))
(set! *main-cli-fn* -main)
