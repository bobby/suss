(ns suss-oracle.sorted-prerequisites)
(def ci-reduce cljs.core/ci-reduce)
(def compare-symbols cljs.core/compare-symbols)
(def compare-keywords cljs.core/compare-keywords)
(deftype SortedIndexedProbe [values trace]
  ICounted
  (-count [_] (swap! trace conj :count) (count values))
  IIndexed
  (-nth [_ n] (swap! trace conj n) (nth values n))
  (-nth [_ n nf] (swap! trace conj n) (nth values n nf)))
(deftype SortedThrowingCount [trace]
  ICounted
  (-count [_] (swap! trace conj :count) (throw 73))
  IIndexed
  (-nth [_ n] (swap! trace conj n) 0)
  (-nth [_ n nf] (swap! trace conj n) nf))

(defn -main [] (println (.stringify js/JSON #js [
#js {:id "symbol-identity" :value (= (compare-symbols (symbol "n" "a") (symbol "n" "a")) 0)}
#js {:id "symbol-no-namespace" :value (= (compare-symbols (symbol "a") (symbol "b")) -1)}
#js {:id "symbol-left-namespace-absent" :value (= (compare-symbols (symbol "z") (symbol "n" "a")) -1)}
#js {:id "symbol-right-namespace-absent" :value (= (compare-symbols (symbol "n" "a") (symbol "z")) 1)}
#js {:id "symbol-namespace-before-name" :value (= (compare-symbols (symbol "a" "z") (symbol "b" "a")) -1)}
#js {:id "symbol-namespace-greater" :value (= (compare-symbols (symbol "z" "a") (symbol "b" "z")) 1)}
#js {:id "symbol-namespace-tie-name" :value (= (compare-symbols (symbol "n" "z") (symbol "n" "a")) 1)}
#js {:id "symbol-empty-name" :value (= (compare-symbols (symbol "n" "") (symbol "n" "a")) -1)}
#js {:id "symbol-utf16" :value (= (compare-symbols (symbol "n" "\ud83d\ude00") (symbol "n" "\ue000")) -1)}
#js {:id "symbol-no-public-comparator" :value (let [c compare gt > lt <] (try (set! compare (fn [_ _] (throw 71))) (set! > (fn ([x] (throw 72)) ([x y] (throw 72)) ([x y & more] (throw 72)))) (set! < (fn ([x] (throw 73)) ([x y] (throw 73)) ([x y & more] (throw 73)))) (= (compare-symbols (symbol "n" "a") (symbol "n" "b")) -1) (finally (set! compare c) (set! > gt) (set! < lt))))}
#js {:id "symbol-identity-skips-fields" :value (= (compare-symbols (js-obj "str" "same") (js-obj "str" "same")) 0)}
#js {:id "symbol-repeated-conversion-order" :value (let [trace (atom []) a (js-obj "str" "a" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 1) "a"))) b (js-obj "str" "b" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 2) "b")))] (and (= (compare-symbols a b) -1) (= @trace [1 2 1 2])))}
#js {:id "symbol-conversion-throw-stops-right" :value (let [trace (atom []) a (js-obj "str" "a" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 1) (throw 73)))) b (js-obj "str" "b" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 2) "b")))] (try (compare-symbols a b) false (catch :default e (and (= e 73) (= @trace [1])))))}
#js {:id "keyword-identity" :value (= (compare-keywords (keyword "n" "a") (keyword "n" "a")) 0)}
#js {:id "keyword-no-namespace" :value (= (compare-keywords (keyword "a") (keyword "b")) -1)}
#js {:id "keyword-left-namespace-absent" :value (= (compare-keywords (keyword "z") (keyword "n" "a")) -1)}
#js {:id "keyword-right-namespace-absent" :value (= (compare-keywords (keyword "n" "a") (keyword "z")) 1)}
#js {:id "keyword-namespace-before-name" :value (= (compare-keywords (keyword "a" "z") (keyword "b" "a")) -1)}
#js {:id "keyword-namespace-greater" :value (= (compare-keywords (keyword "z" "a") (keyword "b" "z")) 1)}
#js {:id "keyword-namespace-tie-name" :value (= (compare-keywords (keyword "n" "z") (keyword "n" "a")) 1)}
#js {:id "keyword-empty-name" :value (= (compare-keywords (keyword "n" "") (keyword "n" "a")) -1)}
#js {:id "keyword-utf16" :value (= (compare-keywords (keyword "n" "\ud83d\ude00") (keyword "n" "\ue000")) -1)}
#js {:id "keyword-no-public-comparator" :value (let [c compare gt > lt <] (try (set! compare (fn [_ _] (throw 71))) (set! > (fn ([x] (throw 72)) ([x y] (throw 72)) ([x y & more] (throw 72)))) (set! < (fn ([x] (throw 73)) ([x y] (throw 73)) ([x y & more] (throw 73)))) (= (compare-keywords (keyword "n" "a") (keyword "n" "b")) -1) (finally (set! compare c) (set! > gt) (set! < lt))))}
#js {:id "keyword-identity-skips-fields" :value (= (compare-keywords (js-obj "fqn" "same") (js-obj "fqn" "same")) 0)}
#js {:id "keyword-repeated-conversion-order" :value (let [trace (atom []) a (js-obj "fqn" "a" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 1) "a"))) b (js-obj "fqn" "b" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 2) "b")))] (and (= (compare-keywords a b) -1) (= @trace [1 2 1 2])))}
#js {:id "keyword-conversion-throw-stops-right" :value (let [trace (atom []) a (js-obj "fqn" "a" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 1) (throw 73)))) b (js-obj "fqn" "b" "ns" nil "name" (js-obj "valueOf" (fn [] (swap! trace conj 2) "b")))] (try (compare-keywords a b) false (catch :default e (and (= e 73) (= @trace [1])))))}
#js {:id "ci-empty-no-init" :value (let [calls (atom 0)] (and (= (ci-reduce [] (fn [] (swap! calls inc) 17)) 17) (= @calls 1)))}
#js {:id "ci-single-seed-no-call" :value (= (ci-reduce [17] (fn [_ _] (throw 73))) 17)}
#js {:id "ci-no-init-ordered" :value (let [trace (atom []) p (SortedIndexedProbe. [1 2 3] trace)] (and (= (ci-reduce p (fn [a b] (swap! trace conj :reduce) (+ a b))) 6) (= @trace [:count 0 1 :reduce 2 :reduce])))}
#js {:id "ci-empty-init-identity" :value (let [x (js-obj)] (identical? (ci-reduce [] (fn [_ _] (throw 73)) x) x))}
#js {:id "ci-init-ordered" :value (let [trace (atom []) p (SortedIndexedProbe. [2 3] trace)] (and (= (ci-reduce p (fn [a b] (swap! trace conj :reduce) (+ a b)) 10) 15) (= @trace [:count 0 :reduce 1 :reduce])))}
#js {:id "ci-reduced-no-init" :value (let [trace (atom []) p (SortedIndexedProbe. [1 2 3] trace)] (and (= (ci-reduce p (fn [_ _] (swap! trace conj :reduce) (reduced 17))) 17) (= @trace [:count 0 1 :reduce])))}
#js {:id "ci-reduced-init-false" :value (let [trace (atom []) p (SortedIndexedProbe. [1 2] trace)] (and (false? (ci-reduce p (fn [_ _] (swap! trace conj :reduce) (reduced false)) 10)) (= @trace [:count 0 :reduce])))}
#js {:id "ci-reduced-init-nil" :value (nil? (ci-reduce [1 2] (fn [_ _] (reduced nil)) 10))}
#js {:id "ci-original-reducer-throw" :value (let [trace (atom []) p (SortedIndexedProbe. [1 2 3] trace)] (try (ci-reduce p (fn [_ _] (swap! trace conj :reduce) (throw 73)) 10) false (catch :default e (and (= e 73) (= @trace [:count 0 :reduce])))))}
#js {:id "ci-live-reduced-predicate" :value (let [saved reduced? calls (atom 0)] (try (set! reduced? (fn [x] (swap! calls inc) false)) (and (= (ci-reduce [1 2 3] + 0) 6) (= @calls 3)) (finally (set! reduced? saved))))}
#js {:id "ci-count-before-nth-throw" :value (let [trace (atom []) p (SortedThrowingCount. trace)] (try (ci-reduce p +) false (catch :default e (and (= e 73) (= @trace [:count])))))}
#js {:id "ci-live-deref" :value (let [saved deref calls (atom 0)] (try (set! deref (fn [_] (swap! calls inc) 23)) (and (= (ci-reduce [1 2] (fn [_ _] (reduced 17))) 23) (= @calls 1)) (finally (set! deref saved))))}
#js {:id "ci-seed-false" :value (false? (ci-reduce [false] (fn [_ _] (throw 73))))}
#js {:id "ci-seed-nil" :value (nil? (ci-reduce [nil] (fn [_ _] (throw 73))))}
])))
(set! *main-cli-fn* -main)
