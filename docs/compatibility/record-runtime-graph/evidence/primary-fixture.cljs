(ns suss-oracle.record-graph-runtime-cases)
(defn observations [encode]
 [#js {:id "comp-zero" :value (encode ((comp) false))}
#js {:id "comp-one" :value (encode ((comp (fn [x] [x])) nil))}
#js {:id "comp-2-args-0" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((comp f1 f2) )] [result @trace]))}
#js {:id "comp-2-args-1" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((comp f1 f2) 0)] [result @trace]))}
#js {:id "comp-2-args-2" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((comp f1 f2) 0 1)] [result @trace]))}
#js {:id "comp-2-args-3" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((comp f1 f2) 0 1 2)] [result @trace]))}
#js {:id "comp-2-args-5" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((comp f1 f2) 0 1 2 3 4)] [result @trace]))}
#js {:id "comp-3-args-0" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((comp f1 f2 f3) )] [result @trace]))}
#js {:id "comp-3-args-1" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((comp f1 f2 f3) 0)] [result @trace]))}
#js {:id "comp-3-args-2" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((comp f1 f2 f3) 0 1)] [result @trace]))}
#js {:id "comp-3-args-3" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((comp f1 f2 f3) 0 1 2)] [result @trace]))}
#js {:id "comp-3-args-5" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((comp f1 f2 f3) 0 1 2 3 4)] [result @trace]))}
#js {:id "comp-5-args-0" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [x] (swap! trace conj 3) [3 x]) f4 (fn [x] (swap! trace conj 4) [4 x]) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((comp f1 f2 f3 f4 f5) )] [result @trace]))}
#js {:id "comp-5-args-1" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [x] (swap! trace conj 3) [3 x]) f4 (fn [x] (swap! trace conj 4) [4 x]) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((comp f1 f2 f3 f4 f5) 0)] [result @trace]))}
#js {:id "comp-5-args-2" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [x] (swap! trace conj 3) [3 x]) f4 (fn [x] (swap! trace conj 4) [4 x]) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((comp f1 f2 f3 f4 f5) 0 1)] [result @trace]))}
#js {:id "comp-5-args-3" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [x] (swap! trace conj 3) [3 x]) f4 (fn [x] (swap! trace conj 4) [4 x]) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((comp f1 f2 f3 f4 f5) 0 1 2)] [result @trace]))}
#js {:id "comp-5-args-5" :value (encode (let [trace (atom []) f1 (fn [x] (swap! trace conj 1) [1 x]) f2 (fn [x] (swap! trace conj 2) [2 x]) f3 (fn [x] (swap! trace conj 3) [3 x]) f4 (fn [x] (swap! trace conj 4) [4 x]) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((comp f1 f2 f3 f4 f5) 0 1 2 3 4)] [result @trace]))}
#js {:id "partial-0-args-0" :value (encode ((partial vector ) ))}
#js {:id "partial-0-args-1" :value (encode ((partial vector ) 0))}
#js {:id "partial-0-args-2" :value (encode ((partial vector ) 0 1))}
#js {:id "partial-0-args-3" :value (encode ((partial vector ) 0 1 2))}
#js {:id "partial-0-args-5" :value (encode ((partial vector ) 0 1 2 3 4))}
#js {:id "partial-1-args-0" :value (encode ((partial vector nil) ))}
#js {:id "partial-1-args-1" :value (encode ((partial vector nil) 0))}
#js {:id "partial-1-args-2" :value (encode ((partial vector nil) 0 1))}
#js {:id "partial-1-args-3" :value (encode ((partial vector nil) 0 1 2))}
#js {:id "partial-1-args-5" :value (encode ((partial vector nil) 0 1 2 3 4))}
#js {:id "partial-2-args-0" :value (encode ((partial vector nil false) ))}
#js {:id "partial-2-args-1" :value (encode ((partial vector nil false) 0))}
#js {:id "partial-2-args-2" :value (encode ((partial vector nil false) 0 1))}
#js {:id "partial-2-args-3" :value (encode ((partial vector nil false) 0 1 2))}
#js {:id "partial-2-args-5" :value (encode ((partial vector nil false) 0 1 2 3 4))}
#js {:id "partial-3-args-0" :value (encode ((partial vector nil false 17) ))}
#js {:id "partial-3-args-1" :value (encode ((partial vector nil false 17) 0))}
#js {:id "partial-3-args-2" :value (encode ((partial vector nil false 17) 0 1))}
#js {:id "partial-3-args-3" :value (encode ((partial vector nil false 17) 0 1 2))}
#js {:id "partial-3-args-5" :value (encode ((partial vector nil false 17) 0 1 2 3 4))}
#js {:id "partial-5-args-0" :value (encode ((partial vector nil false 17 23 29) ))}
#js {:id "partial-5-args-1" :value (encode ((partial vector nil false 17 23 29) 0))}
#js {:id "partial-5-args-2" :value (encode ((partial vector nil false 17 23 29) 0 1))}
#js {:id "partial-5-args-3" :value (encode ((partial vector nil false 17 23 29) 0 1 2))}
#js {:id "partial-5-args-5" :value (encode ((partial vector nil false 17 23 29) 0 1 2 3 4))}
#js {:id "juxt-1-args-0" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) result ((juxt f1) )] [result @trace]))}
#js {:id "juxt-1-args-1" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) result ((juxt f1) 0)] [result @trace]))}
#js {:id "juxt-1-args-2" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) result ((juxt f1) 0 1)] [result @trace]))}
#js {:id "juxt-1-args-3" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) result ((juxt f1) 0 1 2)] [result @trace]))}
#js {:id "juxt-1-args-5" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) result ((juxt f1) 0 1 2 3 4)] [result @trace]))}
#js {:id "juxt-2-args-0" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((juxt f1 f2) )] [result @trace]))}
#js {:id "juxt-2-args-1" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((juxt f1 f2) 0)] [result @trace]))}
#js {:id "juxt-2-args-2" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((juxt f1 f2) 0 1)] [result @trace]))}
#js {:id "juxt-2-args-3" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((juxt f1 f2) 0 1 2)] [result @trace]))}
#js {:id "juxt-2-args-5" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) result ((juxt f1 f2) 0 1 2 3 4)] [result @trace]))}
#js {:id "juxt-3-args-0" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((juxt f1 f2 f3) )] [result @trace]))}
#js {:id "juxt-3-args-1" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((juxt f1 f2 f3) 0)] [result @trace]))}
#js {:id "juxt-3-args-2" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((juxt f1 f2 f3) 0 1)] [result @trace]))}
#js {:id "juxt-3-args-3" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((juxt f1 f2 f3) 0 1 2)] [result @trace]))}
#js {:id "juxt-3-args-5" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) result ((juxt f1 f2 f3) 0 1 2 3 4)] [result @trace]))}
#js {:id "juxt-5-args-0" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) f4 (fn [& xs] (swap! trace conj 4) (apply vector xs)) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((juxt f1 f2 f3 f4 f5) )] [result @trace]))}
#js {:id "juxt-5-args-1" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) f4 (fn [& xs] (swap! trace conj 4) (apply vector xs)) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((juxt f1 f2 f3 f4 f5) 0)] [result @trace]))}
#js {:id "juxt-5-args-2" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) f4 (fn [& xs] (swap! trace conj 4) (apply vector xs)) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((juxt f1 f2 f3 f4 f5) 0 1)] [result @trace]))}
#js {:id "juxt-5-args-3" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) f4 (fn [& xs] (swap! trace conj 4) (apply vector xs)) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((juxt f1 f2 f3 f4 f5) 0 1 2)] [result @trace]))}
#js {:id "juxt-5-args-5" :value (encode (let [trace (atom []) f1 (fn [& xs] (swap! trace conj 1) (apply vector xs)) f2 (fn [& xs] (swap! trace conj 2) (apply vector xs)) f3 (fn [& xs] (swap! trace conj 3) (apply vector xs)) f4 (fn [& xs] (swap! trace conj 4) (apply vector xs)) f5 (fn [& xs] (swap! trace conj 5) (apply vector xs)) result ((juxt f1 f2 f3 f4 f5) 0 1 2 3 4)] [result @trace]))}
#js {:id "merge-zero" :value (encode (merge))}
#js {:id "merge-all-nil" :value (encode (merge nil nil))}
#js {:id "merge-false-nil-values" :value (encode (let [a {:a false :b nil} b {:b 17 :c false} r (merge nil a b)] [(get r :a) (get r :b) (get r :c) (count r) (get a :b)]))}
#js {:id "merge-with-zero" :value (encode (merge-with vector))}
#js {:id "merge-with-order" :value (encode (let [trace (atom []) r (merge-with (fn [a b] (swap! trace conj [a b]) [a b]) {:k false} {:k nil} {:k 17})] [(get r :k) @trace]))}
#js {:id "merge-with-absence" :value (encode (let [trace (atom []) r (merge-with (fn [a b] (swap! trace conj [a b]) [a b]) nil {:k false} {:x nil})] [(get r :k) (get r :x) (count r) @trace]))}
#js {:id "update-args-0" :value (encode (let [m {:v false} trace (atom []) r (update m :v (fn [old & xs] (swap! trace conj old) [old (apply vector xs)]) )] [(get r :v) @trace (get m :v)]))}
#js {:id "update-args-1" :value (encode (let [m {:v false} trace (atom []) r (update m :v (fn [old & xs] (swap! trace conj old) [old (apply vector xs)]) 0)] [(get r :v) @trace (get m :v)]))}
#js {:id "update-args-2" :value (encode (let [m {:v false} trace (atom []) r (update m :v (fn [old & xs] (swap! trace conj old) [old (apply vector xs)]) 0 1)] [(get r :v) @trace (get m :v)]))}
#js {:id "update-args-3" :value (encode (let [m {:v false} trace (atom []) r (update m :v (fn [old & xs] (swap! trace conj old) [old (apply vector xs)]) 0 1 2)] [(get r :v) @trace (get m :v)]))}
#js {:id "update-args-4" :value (encode (let [m {:v false} trace (atom []) r (update m :v (fn [old & xs] (swap! trace conj old) [old (apply vector xs)]) 0 1 2 3)] [(get r :v) @trace (get m :v)]))}
#js {:id "update-missing" :value (encode (let [r (update {} :v vector 17)] [(get r :v)]))}
#js {:id "update-throw-order" :value (encode (let [trace (atom []) caught (atom nil)] (try (update {:v false} :v (fn [old x] (swap! trace conj old) (throw x)) (do (swap! trace conj 1) 73)) (catch :default e (reset! caught e))) [@trace @caught]))}
#js {:id "select-keys-meta-presence" :value (encode (let [m (with-meta {:a nil :b false :c 17} {:stamp false}) r (select-keys m [:a :b :a :missing])] [(count r) (get r :a 99) (get r :b 99) (get r :missing 99) (:stamp (meta r)) (count m)]))}
#js {:id "select-keys-pinned-sentinel" :value (encode (let [r (select-keys {:a :cljs.core/not-found :b false} [:a :b])] [(count r) (get r :a 99) (get r :b)]))}
#js {:id "zipmap-empty" :value (encode (count (zipmap [] [])))}
#js {:id "zipmap-shortest" :value (encode (let [r (zipmap [:a :b :c] [false nil])] [(count r) (get r :a 99) (get r :b 99) (get r :c 99)]))}
#js {:id "zipmap-duplicate" :value (encode (let [r (zipmap [nil false nil] [17 nil 23])] [(count r) (get r nil) (get r false 99)]))}
#js {:id "zipmap-promotion" :value (encode (let [r (zipmap [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16] [20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36])] [(count r) [(get r 0) (get r 1) (get r 2) (get r 3) (get r 4) (get r 5) (get r 6) (get r 7) (get r 8) (get r 9) (get r 10) (get r 11) (get r 12) (get r 13) (get r 14) (get r 15) (get r 16)]]))}
#js {:id "gensym-both-arities-counter" :value (encode (let [old gensym_counter] (try (set! gensym_counter nil) [(str (gensym)) (str (gensym "r")) @gensym_counter] (finally (set! gensym_counter old)))))}
#js {:id "gensym-counter-reuse" :value (encode (let [old gensym_counter counter (atom 9)] (try (set! gensym_counter counter) [(str (gensym "r")) @counter] (finally (set! gensym_counter old)))))}
#js {:id "gensym-immutable-nil-guard" :value (encode (let [old-counter gensym_counter old-nil nil? counter (atom 9)] (try (set! gensym_counter counter) (set! nil? (fn [_] true)) [(str (gensym "r")) @counter] (finally (set! nil? old-nil) (set! gensym_counter old-counter)))))}])
