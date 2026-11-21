(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [^long x (identity 5.5) y (if true x 1)] [x y (+ y 1)])"
"(let [^long x (identity 5.5) y (do x)] [x y (+ y 1)])"
"(let [x (identity 5.5)] [x (+ ^long x 1) x])"
"(let [^long x (identity 5.5)] [(+ ^double x 1) x])"
"(let [^long x (identity 5.5)] [(= x 5.5) (compare x 5.5) (bit-clear x 0) (bit-set x 1)])"
"(let [^double x (identity 5)] [(quot x 2) (rem x 2) (mod x 2) (abs x)])"
"(let [^long x (identity 5.5)] [(long x) (unchecked-long x) (byte x) (short x)])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (+ x 1 (do (swap! events conj :later) 1)) (catch Exception e (ex-message e))) @events])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (< x 1 (do (swap! events conj :later) 1)) (catch Exception e (ex-message e))) @events])"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
