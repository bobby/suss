(assert (= "1.12.6" (clojure-version)))
(prn (deref (future (let [outside ((fn [x] x) ##NaN) inside (with-redefs [*print-length* 7] ((fn [x] x) ##NaN))] (if (identical? outside inside) 1 0)))))
(prn (deref (future (let [outside ((fn [x] x) ##NaN) inside (binding [*print-length* 7] ((fn [x] x) ##NaN))] (if (identical? outside inside) 1 0)))))
(shutdown-agents)
