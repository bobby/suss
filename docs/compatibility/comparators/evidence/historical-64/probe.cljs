(ns comparator-plan.probe (:require [suss-oracle.main :as transport]))
(def effects (atom []))
(deftype CompareProbe [value]
  IComparable
  (-compare [_ other] (swap! effects conj value other) (- value other)))
(deftype ThrowingCompareProbe []
  IComparable
  (-compare [_ other] (throw false)))
(deftype CoerceProbe [identity text]
  Object
  (toString [_] (swap! effects conj identity) text))
(deftype UncomparableProbe [text]
  Object
  (toString [_] text))
(defn -main []
 (println (.stringify js/JSON #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1" :cases #js [
 #js {:id "nil-nil" :value (transport/encode (compare nil nil))}
 #js {:id "nil-false" :value (transport/encode (compare nil false))}
 #js {:id "false-nil" :value (transport/encode (compare false nil))}
 #js {:id "nil-undefined" :value (transport/encode (compare nil js/undefined))}
 #js {:id "undefined-nil" :value (transport/encode (compare js/undefined nil))}
 #js {:id "undefined-undefined" :value (transport/encode (compare js/undefined js/undefined))}
 #js {:id "number-less" :value (transport/encode (compare 2 10))}
 #js {:id "number-greater" :value (transport/encode (compare 10 2))}
 #js {:id "number-equal" :value (transport/encode (compare 2 2))}
 #js {:id "signed-zero" :value (transport/encode (compare -0.0 0.0))}
 #js {:id "nan-self" :value (transport/encode (compare js/NaN js/NaN))}
 #js {:id "nan-left" :value (transport/encode (compare js/NaN 2))}
 #js {:id "nan-right" :value (transport/encode (compare 2 js/NaN))}
 #js {:id "infinity" :value (transport/encode (compare js/Infinity (- js/Infinity)))}
 #js {:id "false-true" :value (transport/encode (compare false true))}
 #js {:id "true-false" :value (transport/encode (compare true false))}
 #js {:id "string-prefix" :value (transport/encode (compare "a" "aa"))}
 #js {:id "string-numeric-text" :value (transport/encode (compare "2" "10"))}
 #js {:id "utf16-astral-bmp" :value (transport/encode (compare "\ud83d\ude00" "\ue000"))}
 #js {:id "utf16-lone" :value (transport/encode (compare "\ud800" "\ud801"))}
 #js {:id "array-empty" :value (transport/encode (compare (array) (array)))}
 #js {:id "array-number-lex" :value (transport/encode (compare (array 2) (array 10)))}
 #js {:id "array-multiple-lex" :value (transport/encode (compare (array 1 2) (array 1 10)))}
 #js {:id "array-nil-undefined" :value (transport/encode (compare (array nil) (array js/undefined)))}
 #js {:id "array-boolean-lex" :value (transport/encode (compare (array true) (array false)))}
 #js {:id "array-nan-lex" :value (transport/encode (compare (array js/NaN) (array js/Infinity)))}
 #js {:id "array-nested" :value (transport/encode (compare (array (array 2 3)) (array 2 3)))}
 #js {:id "array-hole" :value (transport/encode (compare (js/Array. 1) (array nil)))}
 #js {:id "array-cycle" :value (transport/encode (let [a (array)] (aset a 0 a) (compare a (array))))}
 #js {:id "custom-comparable" :value (transport/encode (compare (CompareProbe. 17) 5))}
 #js {:id "custom-comparable-false" :value (transport/encode (compare (CompareProbe. 17) false))}
 #js {:id "same-object-bypasses-dispatch" :value (transport/encode (let [x (CompareProbe. 17)] (reset! effects []) (compare x x) @effects))}
 #js {:id "dispatch-before-type" :value (transport/encode (do (reset! effects []) (compare (CompareProbe. 17) 5) @effects))}
 #js {:id "fn-compare-identity" :value (transport/encode (identical? (cljs.core/fn->comparator compare) compare))}
 #js {:id "fn-numeric-negative" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] -7)) 2 3))}
 #js {:id "fn-numeric-zero" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] -0.0)) 2 3))}
 #js {:id "fn-numeric-nan" :value (transport/encode (js/isNaN ((cljs.core/fn->comparator (fn [x y] js/NaN)) 2 3)))}
 #js {:id "fn-numeric-infinity" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] js/Infinity)) 2 3))}
 #js {:id "fn-true" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] true)) 2 3))}
 #js {:id "fn-false" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] false)) 2 3))}
 #js {:id "fn-nil" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] nil)) 2 3))}
 #js {:id "fn-empty-string-truthy" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] "")) 2 3))}
 #js {:id "fn-empty-array-truthy" :value (transport/encode ((cljs.core/fn->comparator (fn [x y] (array))) 2 3))}
 #js {:id "fn-reverse-number-zero-truthy" :value (transport/encode (let [f (fn [x y] (if (< x y) false 0))] ((cljs.core/fn->comparator f) 2 3)))}
 #js {:id "fn-reverse-nan-truthy" :value (transport/encode (let [f (fn [x y] (if (< x y) false js/NaN))] ((cljs.core/fn->comparator f) 2 3)))}
 #js {:id "fn-predicate-reversal" :value (transport/encode (do (reset! effects []) (let [f (fn [x y] (swap! effects conj x y) (< x y))] ((cljs.core/fn->comparator f) 3 2)) @effects))}
 #js {:id "fn-numeric-once" :value (transport/encode (do (reset! effects []) (let [f (fn [x y] (swap! effects conj x y) 0)] ((cljs.core/fn->comparator f) 3 2)) @effects))}
 #js {:id "array-coercion-both-relations" :value (transport/encode (let [a (array (CoerceProbe. 1 "a")) b (array (CoerceProbe. 2 "z"))] (reset! effects []) (compare a b) @effects))}
 #js {:id "array-coercion-first-relation" :value (transport/encode (let [a (array (CoerceProbe. 1 "z")) b (array (CoerceProbe. 2 "a"))] (reset! effects []) (compare a b) @effects))}
 #js {:id "number-string-error" :value (transport/encode (try (compare 1 "1") (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "string-number-error" :value (transport/encode (try (compare "1" 1) (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "boolean-number-error" :value (transport/encode (try (compare false 0) (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "array-string-error" :value (transport/encode (try (compare (array 1 2) "x") (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "number-array-error" :value (transport/encode (try (compare 1 (array 2 3)) (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "object-error" :value (transport/encode (try (compare (UncomparableProbe. "left") (UncomparableProbe. "right")) (catch js/Error ex [(instance? js/Error ex) (.-message ex)])))}
 #js {:id "error-format-order" :value (transport/encode (do (reset! effects []) (try (compare (CoerceProbe. 1 "left") (CoerceProbe. 2 "right")) (catch js/Error ex nil)) @effects))}
 #js {:id "primitive-relations-ignore-public-rebind" :value (transport/encode (with-redefs [cljs.core/> (fn [x y] false) cljs.core/< (fn [x y] false)] (compare 2 10)))}
 #js {:id "primitive-predicates-ignore-public-rebind" :value (transport/encode (with-redefs [cljs.core/number? (fn [x] false) cljs.core/string? (fn [x] false) cljs.core/identical? (fn [x y] false) cljs.core/nil? (fn [x] false) cljs.core/true? (fn [x] false) cljs.core/false? (fn [x] false)] [(compare 2 10) (compare "a" "z") (compare false true) (compare nil 2)]))}
 #js {:id "array-predicate-stays-live" :value (transport/encode (with-redefs [cljs.core/array? (fn [x] false)] (try (compare (array 2) (array 10)) (catch js/Error ex (.-message ex)))))}
 #js {:id "type-function-stays-live" :value (transport/encode (with-redefs [cljs.core/type (fn [x] nil)] (compare "2" 10)))}
 #js {:id "compare-var-stays-live-for-wrapper" :value (transport/encode (let [f (fn [x y] 123)] (with-redefs [cljs.core/compare f] (identical? (cljs.core/fn->comparator f) f))))}
 #js {:id "custom-thrown-payload" :value (transport/encode (try (compare (ThrowingCompareProbe.) 5) (catch :default ex ex)))}
 #js {:id "indexed-length-before-elements" :value (transport/encode (compare [100] [0 0]))}
 #js {:id "indexed-equal-length-lex" :value (transport/encode (compare [2 3] [2 10]))}] })))
(set! *main-cli-fn* -main)
