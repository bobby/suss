(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^long x (identity 5.5)] [(unchecked-inc x) (unchecked-dec x)])"
"(let [^double x (identity 5)] [(unchecked-inc x) (unchecked-dec x)])"
"[(unchecked-inc (identity 9223372036854775807)) (unchecked-dec (identity -9223372036854775808))]"
"(let [^long x (identity 5.5)] [(max x 1) (min x 1) (max x) (min x) (max x 1 2)])"
"(let [^double x (identity -0.0)] [(/ 1.0 (max x 0.0)) (/ 1.0 (max 0.0 x)) (/ 1.0 (min x 0.0)) (/ 1.0 (min 0.0 x))])"
"(let [^double x (identity -0.0)] [(/ 1.0 (max x (identity 0.0))) (/ 1.0 (max (identity 0.0) x)) (/ 1.0 (min x (identity 0.0))) (/ 1.0 (min (identity 0.0) x))])"
"(let [^long x (identity 5.5) ^double y (identity 6)] [(max x y) (min x y) (max x (identity 6)) (min x (identity 6))])"
"(let [^double x (identity ##NaN)] [(max x 1.0) (min 1.0 x) (max x (identity 1)) (min (identity 1) x)])"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
