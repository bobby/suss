(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^long x (identity 5.5)] [(max x 1) (min x 1) (max x) (min x) (max x 1 2)])"
"(let [^long x (identity 5.5)] [(unchecked-inc x) (unchecked-dec x) (unchecked-negate x) (unchecked-add x 1)])"
"(let [^long x (identity 5.5)] [(max x (identity 6)) (min x (identity 6))])"
"(fn ^double [] (max 1 2))"
"(fn ^long [] (max (identity 1) 2.0))"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
