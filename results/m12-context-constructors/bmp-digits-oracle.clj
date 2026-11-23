(assert (= "1.12.6" (clojure-version)))
(doseq [unit (range 65536)]
  (let [digit (Character/digit (char unit) 10)]
    (when (>= digit 0) (println (str "[" unit "," digit "]")))))
