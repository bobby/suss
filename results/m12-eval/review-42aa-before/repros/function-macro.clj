(assert (= "1.12.6" (clojure-version)))
(prn (do (eval '(defmacro passthrough [x] x)) (let [f (fn [] 42)] ((eval (list 'passthrough f))))))
