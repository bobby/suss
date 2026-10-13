(ns suss-oracle.comparators)
(def fn->comparator cljs.core/fn->comparator)
(def compare-indexed cljs.core/compare-indexed)
(def str_ cljs.core/str_)
;; Original portable probes for the complete comparator forms; not upstream code.
(deftype ComparatorProbe [value]
  Object
  (toString [_] (str_ value))
  IComparable
  (-compare [_ other] (- value other)))
(deftype ThrowingComparatorProbe []
  IComparable
  (-compare [_ other] (throw false)))

(defn -main [] (println (.stringify js/JSON #js [
#js {:id "identity-object" :value (let [x (js-obj "toString" (fn [] (throw 17)))] (= (compare x x) 0))}
#js {:id "nil-order" :value (and (= (compare nil false) -1) (= (compare false nil) 1) (= (compare nil nil) 0))}
#js {:id "distinct-nil-undefined" :value (let [u (aget (array) 0)] (and (= (compare nil u) -1) (= (compare u nil) -1) (= (compare u u) 0)))}
#js {:id "number-nan" :value (and (= (compare ##NaN ##NaN) 0) (= (compare ##NaN 2) 0) (= (compare 2 ##NaN) 0))}
#js {:id "number-infinity" :value (and (= (compare ##Inf ##-Inf) 1) (= (compare -0.0 0.0) 0))}
#js {:id "numeric-order" :value (and (= (compare 2 10) -1) (= (compare 10 2) 1))}
#js {:id "utf16-order" :value (and (= (compare "\ud83d\ude00" "\ue000") -1) (= (compare "\ud800" "\ud801") -1) (= (compare "a" "aa") -1))}
#js {:id "boolean-order" :value (and (= (compare false true) -1) (= (compare true false) 1))}
#js {:id "arrays-lexical" :value (and (= (compare (array 2) (array 10)) 1) (= (compare (array 1 2) (array 1 10)) 1))}
#js {:id "array-holes-nil-undefined" :value (let [u (aget (array) 0)] (and (= (compare (make-array 1) (array nil)) 0) (= (compare (array nil) (array u)) 0)))}
#js {:id "array-nested-cycle" :value (let [a (array)] (aset a 0 a) (and (= (compare a (array)) 0) (= (compare (array (array 2 3)) (array 2 3)) 0)))}
#js {:id "comparable-return-not-normalized" :value (let [p (ComparatorProbe. 17)] (= (compare p 5) 12))}
#js {:id "comparable-identity-no-call" :value (let [p (ThrowingComparatorProbe.)] (= (compare p p) 0))}
#js {:id "comparable-original-false" :value (try (compare (ThrowingComparatorProbe.) 5) false (catch :default e (false? e)))}
#js {:id "numeric-before-comparable" :value (try (compare 5 (ComparatorProbe. 17)) false (catch :default e (= (.-message e) "Cannot compare 5 to 17")))}
#js {:id "array-reconvert-both-relations" :value (let [trace (atom []) a (array (js-obj "toString" (fn [] (swap! trace conj 1) "1"))) b (array (js-obj "toString" (fn [] (swap! trace conj 2) "2")))] (and (= (compare a b) -1) (= @trace [1 2 1 2])))}
#js {:id "array-first-relation-only" :value (let [trace (atom []) a (array (js-obj "toString" (fn [] (swap! trace conj 1) "2"))) b (array (js-obj "toString" (fn [] (swap! trace conj 2) "1")))] (and (= (compare a b) 1) (= @trace [1 2])))}
#js {:id "array-conversion-changes-between-tests" :value (let [calls (atom 0) a (array (js-obj "toString" (fn [] (if (= (swap! calls inc) 1) "0" "9"))))] (and (= (compare a (array 1)) 0) (= @calls 2)))}
#js {:id "array-override-valueOf" :value (let [a (array 9) b (array 1)] (aset a "valueOf" (fn [] 0)) (= (compare a b) -1))}
#js {:id "array-type-constructor-nil" :value (let [a (array 2) b (array 10)] (aset a "constructor" nil) (aset b "constructor" nil) (and (nil? (type a)) (= (compare a b) 1)))}
#js {:id "array-type-constructor-false" :value (let [a (array)] (aset a "constructor" false) (false? (type a)))}
#js {:id "array-type-constructor-mismatch" :value (let [a (array 2) b (array 10)] (aset a "constructor" false) (try (compare a b) false (catch :default e (= (.-message e) "Cannot compare 2 to 10"))))}
#js {:id "live-type-mixed" :value (let [saved type] (try (set! type (fn [_] nil)) (= (compare "2" 10) -1) (finally (set! type saved))))}
#js {:id "live-array-predicate" :value (let [saved array?] (try (set! array? (fn [_] false)) (try (compare (array 2) (array 10)) false (catch :default e (= (.-message e) "Cannot compare 2 to 10"))) (finally (set! array? saved))))}
#js {:id "public-primitive-predicate-rebind" :value (let [n number? s string? z nil? t true? f false? i identical?] (try (set! number? (fn [_] false)) (set! string? (fn [_] false)) (set! nil? (fn [_] false)) (set! true? (fn [_] false)) (set! false? (fn [_] false)) (set! identical? (fn [_ _] false)) (and (= (compare 2 10) -1) (= (compare "2" "10") 1) (= (compare nil 2) -1) (= (compare false true) -1)) (finally (set! number? n) (set! string? s) (set! nil? z) (set! true? t) (set! false? f) (set! identical? i))))}
#js {:id "wrapper-number-unchanged" :value (= ((fn->comparator (fn [_ _] -7)) 2 3) -7)}
#js {:id "wrapper-truthy-empty" :value (and (= ((fn->comparator (fn [_ _] "")) 2 3) -1) (= ((fn->comparator (fn [_ _] (array))) 2 3) -1))}
#js {:id "wrapper-false-nil-undefined" :value (and (= ((fn->comparator (fn [_ _] false)) 2 3) 0) (= ((fn->comparator (fn [_ _] nil)) 2 3) 0) (= ((fn->comparator (fn [_ _] (aget (array) 0))) 2 3) 0))}
#js {:id "wrapper-reverse-zero-truthy" :value (let [n (atom 0)] (and (= ((fn->comparator (fn [_ _] (if (= (swap! n inc) 1) false 0))) 2 3) 1) (= @n 2)))}
#js {:id "wrapper-reverse-nan-truthy" :value (let [n (atom 0)] (= ((fn->comparator (fn [_ _] (if (= (swap! n inc) 1) nil ##NaN))) 2 3) 1))}
#js {:id "wrapper-reverse-effects" :value (let [trace (atom []) f (fn [x y] (swap! trace conj x y) (< x y))] (and (= ((fn->comparator f) 3 2) 1) (= @trace [3 2 2 3])))}
#js {:id "wrapper-number-once" :value (let [n (atom 0)] (and (= ((fn->comparator (fn [_ _] (swap! n inc) 0)) 2 3) 0) (= @n 1)))}
#js {:id "wrapper-first-throw-no-reverse" :value (let [n (atom 0)] (try ((fn->comparator (fn [_ _] (swap! n inc) (throw nil))) 2 3) false (catch :default e (and (nil? e) (= @n 1)))))}
#js {:id "wrapper-reverse-throw" :value (let [n (atom 0)] (try ((fn->comparator (fn [_ _] (if (= (swap! n inc) 1) false (throw 17)))) 2 3) false (catch :default e (and (= e 17) (= @n 2)))))}
#js {:id "wrapper-live-compare" :value (let [saved compare f (fn [_ _] 17)] (try (set! compare f) (identical? (fn->comparator f) f) (finally (set! compare saved))))}
#js {:id "wrapper-equality-changes-compare" :value (let [saved-eq = saved-cmp compare replacement (fn [_ _] 17)] (try (set! = (fn ([x] true) ([x y] (set! compare replacement) true) ([x y & more] false))) (identical? (fn->comparator (fn [_ _] 0)) replacement) (finally (set! = saved-eq) (set! compare saved-cmp))))}
#js {:id "indexed-length-first" :value (and (= (compare-indexed [100] [0 0]) -1) (= (compare-indexed [] []) 0) (= (compare-indexed [0 0] [100]) 1))}
#js {:id "indexed-four-arity" :value (and (= (compare-indexed [99 2 3] [0 2 10] 3 1) -1) (= (compare-indexed [2 3] [2 10]) -1))}
#js {:id "indexed-stops-first-difference" :value (let [trace (atom []) saved nth] (try (set! nth (fn ([xs n] (swap! trace conj n) (saved xs n)) ([xs n nf] (saved xs n nf)))) (and (= (compare-indexed [1 9] [2 0]) -1) (= @trace [0 0])) (finally (set! nth saved))))}
#js {:id "indexed-live-compare" :value (let [saved compare trace (atom [])] (try (set! compare (fn [x y] (swap! trace conj x y) 17)) (and (= (compare-indexed [1] [2]) 17) (= @trace [1 2])) (finally (set! compare saved))))}
#js {:id "format-toString-object-fallback" :value (let [trace (atom []) x (js-obj "toString" (fn [] (swap! trace conj :string) (js-obj)) "valueOf" (fn [] (swap! trace conj :value) "left"))] (try (compare x "right") false (catch :default e (and (= (.-message e) "Cannot compare left to right") (= @trace [:string :value])))))}
#js {:id "format-noncallable-toString" :value (let [x (js-obj "toString" 17 "valueOf" (fn [] "left"))] (try (compare x "right") false (catch :default e (= (.-message e) "Cannot compare left to right"))))}
#js {:id "format-order" :value (let [trace (atom []) x (js-obj "toString" (fn [] (swap! trace conj 1) "left")) y (js-obj "toString" (fn [] (swap! trace conj 2) "right"))] (try (compare x y) false (catch :default e (and (= (.-message e) "Cannot compare left to right") (= @trace [1 2])))))}
#js {:id "format-throw-stops-second" :value (let [trace (atom []) x (js-obj "toString" (fn [] (swap! trace conj 1) (throw 73))) y (js-obj "toString" (fn [] (swap! trace conj 2) "right"))] (try (compare x y) false (catch :default e (and (= e 73) (= @trace [1])))))}
#js {:id "format-live-str" :value (let [saved cljs.core/str_ trace (atom [])] (try (set! cljs.core/str_ (fn [x] (swap! trace conj x) "z")) (try (compare 1 "x") false (catch :default e (and (= (.-message e) "Cannot compare z to z") (= @trace [1 "x"])))) (finally (set! cljs.core/str_ saved))))}
#js {:id "str-nil-single-variadic-rebind" :value (let [saved nil?] (try (set! nil? (fn [_] false)) (and (= (str_ nil) "") (= (str_ nil "ignored") "") (= (str_ "x" nil "y") "xy")) (finally (set! nil? saved))))}
#js {:id "str-second-lookup-after-first" :value (let [trace (atom []) x (js-obj)] (set! (.-toString x) (fn [] (swap! trace conj 1) (set! (.-valueOf x) (fn [] (swap! trace conj 2) "ok")) (js-obj))) (and (= (str_ x) "ok") (= @trace [1 2])))}
#js {:id "str-nonprimitive-both-throw" :value (let [trace (atom []) x (js-obj "toString" (fn [] (swap! trace conj 1) (js-obj)) "valueOf" (fn [] (swap! trace conj 2) (js-obj)))] (try (str_ x) false (catch :default e (= @trace [1 2]))))}
#js {:id "format-live-str-results-convert-after-calls" :value (let [saved cljs.core/str_ trace (atom [])] (try (set! cljs.core/str_ (fn [x] (swap! trace conj x) (js-obj "toString" (fn [] (swap! trace conj 17) "z")))) (try (compare 1 "x") false (catch :default e (and (= (.-message e) "Cannot compare z to z") (= @trace [1 "x" 17 17])))) (finally (set! cljs.core/str_ saved))))}
])))
(set! *main-cli-fn* -main)
