(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^double x (identity 5) ^double y (identity 1)] [(bit-and x 1) (= (bit-and x 1) y)])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (+ x (zero? (do (swap! events conj :later) 0))) (catch Throwable e :number-error)) @events])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (< x (zero? (do (swap! events conj :later) 0))) (catch Throwable e :number-error)) @events])"
"(fn ^double [] (let [^double x (identity 5)] (bit-and x 1)))"
"(fn ^double [] (bit-and (identity 5) 1))"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
