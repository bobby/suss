(assert (= "1.12.6" (clojure-version)))
(load-file "results/m12-numeric-captures/bench.sus")
(doseq [name ['capturebench/case-raw 'capturebench/case-boxed 'capturebench/case-direct]
        n [10 1000000 2000000]]
  (let [actual ((resolve name) n) expected (+ (/ (* n (dec n)) 2) (* 1.25 n))]
    (prn name n actual expected)
    (assert (== expected actual))))
