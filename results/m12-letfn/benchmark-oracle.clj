(assert (= "1.12.6" (clojure-version)))
(load-file "results/m12-letfn/bench.sus")
(doseq [name ['letfnbench/case-group 'letfnbench/case-direct]
        n [10 200000 400000]]
  (let [actual ((resolve name) n) expected (+ (/ (* n (dec n)) 2) (* 1.25 n))]
    (prn name n actual expected)
    (assert (== expected actual))))
