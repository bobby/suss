(ns suss-oracle.record-helpers)
(defn -main [] (println (.stringify js/JSON (into-array [#js {:id "vary-meta-args-0" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) )] (and (= result [17]) (= (meta result) {:old false :args []}) (= (meta v) {:old false})))}
#js {:id "vary-meta-args-1" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1)] (and (= result [17]) (= (meta result) {:old false :args [1]}) (= (meta v) {:old false})))}
#js {:id "vary-meta-args-2" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2)] (and (= result [17]) (= (meta result) {:old false :args [1 2]}) (= (meta v) {:old false})))}
#js {:id "vary-meta-args-3" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3)] (and (= result [17]) (= (meta result) {:old false :args [1 2 3]}) (= (meta v) {:old false})))}
#js {:id "vary-meta-args-4" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3 4)] (and (= result [17]) (= (meta result) {:old false :args [1 2 3 4]}) (= (meta v) {:old false})))}
#js {:id "vary-meta-args-5" :value (let [v (with-meta [17] {:old false}) result (vary-meta v (fn [m & xs] {:old (:old m) :args (apply vector xs)}) 1 2 3 4 5)] (and (= result [17]) (= (meta result) {:old false :args [1 2 3 4 5]}) (= (meta v) {:old false})))}
#js {:id "vary-meta-effect-once" :value (let [calls (atom []) v (with-meta [1] {:a 2}) r (vary-meta v (fn [m x] (swap! calls conj m) {:v x}) false)] (and (= @calls [{:a 2}]) (= (meta r) {:v false}) (= (meta v) {:a 2})))}
#js {:id "fnil-defaults-1-args-1" :value (= ((fnil vector 10) nil) [10])}
#js {:id "fnil-defaults-1-args-2" :value (= ((fnil vector 10) nil 2) [10 2])}
#js {:id "fnil-defaults-1-args-3" :value (= ((fnil vector 10) nil 2 3) [10 2 3])}
#js {:id "fnil-defaults-1-args-5" :value (= ((fnil vector 10) nil 2 3 4 5) [10 2 3 4 5])}
#js {:id "fnil-defaults-2-args-2" :value (= ((fnil vector 10 11) nil nil) [10 11])}
#js {:id "fnil-defaults-2-args-3" :value (= ((fnil vector 10 11) nil nil 3) [10 11 3])}
#js {:id "fnil-defaults-2-args-5" :value (= ((fnil vector 10 11) nil nil 3 4 5) [10 11 3 4 5])}
#js {:id "fnil-defaults-3-args-2" :value (= ((fnil vector 10 11 12) nil nil) [10 11])}
#js {:id "fnil-defaults-3-args-3" :value (= ((fnil vector 10 11 12) nil nil nil) [10 11 12])}
#js {:id "fnil-defaults-3-args-5" :value (= ((fnil vector 10 11 12) nil nil nil 4 5) [10 11 12 4 5])}
#js {:id "fnil-false-values" :value (= ((fnil vector 10 11 12) false false false 4) [false false false 4])}
#js {:id "fnil-argument-order-and-once" :value (let [calls (atom []) f (fnil (fn [a b] (swap! calls conj :call) [a b]) 9) result (f (do (swap! calls conj :a) nil) (do (swap! calls conj :b) false))] (and (= result [9 false]) (= @calls [:a :b :call])))}
#js {:id "fnil-immutable-nil-macro" :value (let [old nil? f (fnil vector 9)] (try (set! nil? (fn [_] true)) (and (= (f false) [false]) (= (f nil) [9])) (finally (set! nil? old))))}
#js {:id "update-in-args-0" :value (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) )] (and (= result {:outer {:v [false []]}}) (= m {:outer {:v false}})))}
#js {:id "update-in-args-1" :value (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1)] (and (= result {:outer {:v [false [1]]}}) (= m {:outer {:v false}})))}
#js {:id "update-in-args-2" :value (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2)] (and (= result {:outer {:v [false [1 2]]}}) (= m {:outer {:v false}})))}
#js {:id "update-in-args-3" :value (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2 3)] (and (= result {:outer {:v [false [1 2 3]]}}) (= m {:outer {:v false}})))}
#js {:id "update-in-args-4" :value (let [m {:outer {:v false}} result (update-in m [:outer :v] (fn [old & xs] [old (apply vector xs)]) 1 2 3 4)] (and (= result {:outer {:v [false [1 2 3 4]]}}) (= m {:outer {:v false}})))}
#js {:id "update-in-missing-path" :value (= (update-in nil [:a :b] (fn [v] (if (nil? v) 17 0))) {:a {:b 17}})}
#js {:id "update-in-empty-path" :value (= (update-in {} [] (fn [v] (if (nil? v) 17 0))) {nil 17})}
#js {:id "update-in-throw-order" :value (let [trace (atom []) caught (atom nil)] (try (update-in {:a {:b 3}} [:a :b] (fn [v x] (swap! trace conj v) (throw x)) (do (swap! trace conj :arg) 17)) (catch :default e (reset! caught e))) (and (= @caught 17) (= @trace [:arg 3])))}
#js {:id "group-by-empty" :value (= (group-by identity []) {})}
#js {:id "group-by-nil-false-keys" :value (let [m (group-by identity [nil false nil false])] (and (= (get m nil) [nil nil]) (= (get m false) [false false]) (= (count m) 2)))}
#js {:id "group-by-callback-order-once" :value (let [trace (atom []) v [3 1 2 3] m (group-by (fn [x] (swap! trace conj x) (if (> x 1) :large :small)) v)] (and (= @trace v) (= (:large m) [3 2 3]) (= (:small m) [1]) (= v [3 1 2 3])))}
#js {:id "group-by-transient-promotion" :value (let [m (group-by identity [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16])] (and (= (count m) 17) (= (get m 16) [16]) (= (get m 0) [0])))}
#js {:id "group-by-callback-throw-order" :value (let [trace (atom []) caught (atom nil)] (try (group-by (fn [x] (swap! trace conj x) (if (= x 2) (throw 17) x)) [1 2 3]) (catch :default e (reset! caught e))) (and (= @trace [1 2]) (= @caught 17)))}]))))
(set! *main-cli-fn* -main)
