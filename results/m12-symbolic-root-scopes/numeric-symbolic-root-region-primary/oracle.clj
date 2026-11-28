(assert (= "1.12.6" (clojure-version)))
(prn (let [f (fn [x] (if x ##NaN (with-redefs [*print-length* 3] ##NaN)))] (identical? (f true) (f false))))
(prn (let [f (fn [] (with-redefs [*print-length* 3] ##NaN))] (identical? (f) (f))))
(prn (let [f (fn [x] (if x ##NaN (binding [*print-length* 3] ##NaN)))] (identical? (f true) (f false))))
