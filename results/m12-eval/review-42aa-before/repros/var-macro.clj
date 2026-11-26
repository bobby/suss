(assert (= "1.12.6" (clojure-version)))
(prn (do (eval '(defmacro passthrough [x] x)) (let [v #'identity] (identical? v (eval (list 'passthrough v))))))
