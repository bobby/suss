(assert (= "1.12.6" (clojure-version)))
(prn (let [f (fn [] 42)] ((eval (list 'when-let ['x f] 'x)))))
