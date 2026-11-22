(assert (= "1.12.6" (clojure-version)))
(let [source "(let [x 3000000000] (letfn [(f [n] (if (zero? n) (+ x 1) (g (dec n)))) (g [n] (if (zero? n) (+ x 2) (f (dec n))))] [(f 0) (f 1) (g 1)]))" actual (pr-str (eval (read-string source)))] (prn source actual) (assert (= "[3000000001 3000000002 3000000001]" actual)))
