(assert (= "1.12.6" (clojure-version)))
(prn (let [d (delay 7)] (identical? d (eval d))))
