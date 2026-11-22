(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^long x (identity 1.0E30) events (atom [])] [(try (max x 1 (do (swap! events conj :later) 1)) (catch Throwable e :cast-error)) @events])"
"(let [^long x (identity 5.5)] [(max x 6) (max x 6.0) (max x 1 2.0) (min x 6.0)])"
"((fn ^long [] (max 1 2.0)))"
"(fn ^long [] (max 1.0 2.0))"
"(fn ^double [] (max 1 2))"
"(let [^long x (identity 5.5)] [(max x 1 6) (min x 6 1) (max x (identity 6) 7.0)])"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
