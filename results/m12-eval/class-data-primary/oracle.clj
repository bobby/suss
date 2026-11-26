(assert (= "1.12.6" (clojure-version)))
(prn (mapv (fn [x] [(identical? (type x) (eval (type x))) (eval (list 'instance? (type x) x))]) [42 1.0 9223372036854775808N 1/3 1.50M (biginteger 7)]))
