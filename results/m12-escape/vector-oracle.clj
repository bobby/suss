(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(with-redefs [nth (fn [& xs] 7)] (let [[a b] [1 2]] (+ a b)))"
                "(with-redefs [vector (fn [& xs] nil)] (let [[a b] [1 2]] (+ a b)))"
                "(let [events (atom []) [a b] [(do (swap! events conj :a) 1) (do (swap! events conj :b) 2)]] [(+ a b) @events])"]]
  (prn source (pr-str (eval (read-string source)))))
