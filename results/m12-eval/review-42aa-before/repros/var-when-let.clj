(assert (= "1.12.6" (clojure-version)))
(prn (let [v #'identity] (identical? v (eval (list 'when-let ['x v] 'x)))))
