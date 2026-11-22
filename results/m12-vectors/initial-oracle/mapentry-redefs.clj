(assert (= 3 (with-redefs [nth (fn [& xs] 7)] (let [[a b] (clojure.lang.MapEntry/create 1 2)] (+ a b)))))
(println "MapEntry nth-redefinition assertion passed")
