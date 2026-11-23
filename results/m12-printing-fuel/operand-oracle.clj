(assert (= "1.12.6" (clojure-version)))
(println (pr-str (pr-str (/ 2))))
(println (pr-str (pr-str (let [f /] (f -4)))))
(println (pr-str (pr-str [(= 0.5 (/ 2)) (== 0.5 (/ 2)) (= 1/2 (/ 2)) (= -0.25 (let [f /] (f -4.0)))])))
