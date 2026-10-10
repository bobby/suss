(ns suss-oracle.record-helper-raw-cases)
(defn observations [encode]
 [#js {:id "vary-meta-args-0" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) )] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-args-1" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1)] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-args-2" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2)] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-args-3" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3)] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-args-4" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3 4)] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-args-5" :value (encode (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3 4 5)] [(:old (meta result)) (:args (meta result)) (:old (meta v)) (with-meta result nil)]))}
#js {:id "vary-meta-effect-once" :value (encode (let [calls (atom []) v (with-meta [1] {:a 2}) r (vary-meta v (fn [m x] (swap! calls conj m) {:v x}) false)] [(with-meta r nil) (:v (meta r)) (:a (first @calls)) (count @calls) (:a (meta v))]))}
#js {:id "fnil-defaults-1-args-1" :value (encode ((fnil vector 10) nil))}
#js {:id "fnil-defaults-1-args-2" :value (encode ((fnil vector 10) nil 2))}
#js {:id "fnil-defaults-1-args-3" :value (encode ((fnil vector 10) nil 2 3))}
#js {:id "fnil-defaults-1-args-5" :value (encode ((fnil vector 10) nil 2 3 4 5))}
#js {:id "fnil-defaults-2-args-2" :value (encode ((fnil vector 10 11) nil nil))}
#js {:id "fnil-defaults-2-args-3" :value (encode ((fnil vector 10 11) nil nil 3))}
#js {:id "fnil-defaults-2-args-5" :value (encode ((fnil vector 10 11) nil nil 3 4 5))}
#js {:id "fnil-defaults-3-args-2" :value (encode ((fnil vector 10 11 12) nil nil))}
#js {:id "fnil-defaults-3-args-3" :value (encode ((fnil vector 10 11 12) nil nil nil))}
#js {:id "fnil-defaults-3-args-5" :value (encode ((fnil vector 10 11 12) nil nil nil 4 5))}
#js {:id "fnil-false-values" :value (encode ((fnil vector 10 11 12) false false false 4))}
#js {:id "fnil-argument-order-and-once" :value (encode (let [calls (atom []) f (fnil (fn [a b] (swap! calls conj :call) [a b]) 9) result (f (do (swap! calls conj :a) nil) (do (swap! calls conj :b) false))] [result @calls]))}
#js {:id "fnil-immutable-nil-macro" :value (encode (let [old nil? f (fnil vector 9)] (try (set! nil? (fn [_] true)) [(f false) (f nil)] (finally (set! nil? old)))))}
#js {:id "update-in-args-0" :value (encode (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) )] [(:v (:outer result)) (:v (:outer m))]))}
#js {:id "update-in-args-1" :value (encode (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1)] [(:v (:outer result)) (:v (:outer m))]))}
#js {:id "update-in-args-2" :value (encode (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2)] [(:v (:outer result)) (:v (:outer m))]))}
#js {:id "update-in-args-3" :value (encode (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2 3)] [(:v (:outer result)) (:v (:outer m))]))}
#js {:id "update-in-args-4" :value (encode (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2 3 4)] [(:v (:outer result)) (:v (:outer m))]))}
#js {:id "update-in-missing-path" :value (encode (let [result (update-in nil [:a :b] (fn [v] (if (nil? v) 17 0)))] [(:b (:a result))]))}
#js {:id "update-in-empty-path" :value (encode (let [result (update-in {} [] (fn [v] (if (nil? v) 17 0)))] [(get result nil)]))}
#js {:id "update-in-throw-order" :value (encode (let [trace (atom []) caught (atom nil)] (try (update-in {:a {:b 3}} [:a :b] (fn [v x] (swap! trace conj v) (throw x)) (do (swap! trace conj :arg) 17)) (catch :default e (reset! caught e))) [@caught @trace]))}
#js {:id "group-by-empty" :value (encode (group-by identity []))}
#js {:id "group-by-nil-false-keys" :value (encode (let [m (group-by identity [nil false nil false])] [(get m nil) (get m false) (count m)]))}
#js {:id "group-by-callback-order-once" :value (encode (let [trace (atom []) v [3 1 2 3] m (group-by (fn [x] (swap! trace conj x) (if (> x 1) :large :small)) v)] [@trace (:large m) (:small m) v]))}
#js {:id "group-by-transient-promotion" :value (encode (let [m (group-by identity [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16])] [(count m) [(get m 0) (get m 1) (get m 2) (get m 3) (get m 4) (get m 5) (get m 6) (get m 7) (get m 8) (get m 9) (get m 10) (get m 11) (get m 12) (get m 13) (get m 14) (get m 15) (get m 16)]]))}
#js {:id "group-by-callback-throw-order" :value (encode (let [trace (atom []) caught (atom nil)] (try (group-by (fn [x] (swap! trace conj x) (if (= x 2) (throw 17) x)) [1 2 3]) (catch :default e (reset! caught e))) [@trace @caught]))}])
