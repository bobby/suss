(ns suss-oracle.object-coercion)
;; Original object coercion reference forms.
(defn -main [] (println (.stringify js/JSON (into-array [
#js {:id "number-hint-valueof" :value (< (js-obj "valueOf" (fn [] 2)) 3)}
#js {:id "both-string-relational" :value (not (< (js-obj "valueOf" (fn [] "2")) "10"))}
#js {:id "left-before-right" :value (let [trace (atom []) x (js-obj "valueOf" (fn [] (swap! trace conj :x) 2)) y (js-obj "valueOf" (fn [] (swap! trace conj :y) 3))] (and (< x y) (= @trace [:x :y])))}
#js {:id "property-key-string-hint" :value (let [trace (atom []) a (array) key (js-obj "toString" (fn [] (swap! trace conj :string) "label") "valueOf" (fn [] (swap! trace conj :number) 7))] (aset a key 23) (and (= (aget a "label") 23) (= @trace [:string])))}
#js {:id "object-result-second-method" :value (let [trace (atom []) x (js-obj "valueOf" (fn [] (swap! trace conj :number) (js-obj)) "toString" (fn [] (swap! trace conj :string) "2"))] (and (< x 3) (= @trace [:number :string])))}
#js {:id "noncallable-first-skipped" :value (let [a (array) key (js-obj "toString" 17 "valueOf" (fn [] 7))] (aset a key 23) (= (aget a 7) 23))}
#js {:id "addition-string-result" :value (= (+ (js-obj "valueOf" (fn [] "2")) 3) "23")}
#js {:id "throw-before-right-coercion" :value (let [trace (atom []) x (js-obj "valueOf" (fn [] (swap! trace conj :x) (throw 17))) y (js-obj "valueOf" (fn [] (swap! trace conj :y) 3))] (try (< x y) false (catch :default e (and (= e 17) (= @trace [:x])))))}
]))))
(set! *main-cli-fn* -main)
