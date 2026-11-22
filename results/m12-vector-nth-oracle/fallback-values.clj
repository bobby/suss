(prn (pr-str (with-redefs [nth (fn [& xs] 7)] (nth (list 3000000000 1.5) 0))))
(prn (pr-str (with-redefs [nth (fn [& xs] 7)] (str (nth "ab" 1)))))
(prn (pr-str (with-redefs [nth (fn [& xs] 7)] (nth [1 2] 4 :nf))))
(prn (pr-str (with-redefs [nth (fn [& xs] 7)] (let [[a b] (clojure.lang.MapEntry/create 1 2)] (+ a b)))))
