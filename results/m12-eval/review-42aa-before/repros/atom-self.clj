(assert (= "1.12.6" (clojure-version)))
(prn (let [a (atom 7)] (identical? a (eval a))))
