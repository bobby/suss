(assert (= "1.12.6" (clojure-version)))
(prn (deref (future (let [outside [##NaN] inside (with-redefs [*print-length* 7] [##NaN])] (if (identical? (nth outside 0) (nth inside 0)) 1 0)))))
(prn (deref (future (let [outside [##NaN] inside (binding [*print-length* 7] [##NaN])] (if (identical? (nth outside 0) (nth inside 0)) 1 0)))))
(shutdown-agents)
