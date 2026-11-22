(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^long x (identity 5.5)] [(= x 5) (= x 5.5) (= x (identity 5)) (= x (identity 5.5))])"
"(let [^double x (identity 5)] [(= x 5.0) (= x 5) (= x (identity 5.0)) (= x (identity 5))])"
"(let [^long x (identity 5.5) ^long y (identity 5.0)] (= x y))"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
