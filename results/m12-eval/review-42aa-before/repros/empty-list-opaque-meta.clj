(assert (= "1.12.6" (clojure-version)))
(prn (let [v (with-meta '() {:a (atom 7)})] (identical? v (eval v))))
