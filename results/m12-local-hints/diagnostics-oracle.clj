(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [x 1] ^double x)"
"(let [x 1] ^long x)"
"(let [^long x 1] (+ x 1))"
"(let [^double x 1.0] (+ x 1))"
"(let [x (double 1) ^long y x] y)"
"(loop [x 1.0 n 0] (if (zero? n) (recur (identity x) 1) (let [^long y x] y)))"
"(let [^long x (identity 5.5) y (if true x 1.0)] [y (+ y 1)])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (+ x 1 (do (swap! events conj :later) 1)) (catch Exception e :cast-error)) @events])"
"(let [^long x (identity 1.0E30) events (atom [])] [(try (< x 1 (do (swap! events conj :later) 1)) (catch Exception e :cast-error)) @events])"
"(let [^double x (identity -0.0)] [x (+ x 0) (/ 1 x)])"
"(let [^double x (identity ##NaN)] [(double? (+ x 1)) (= x x) (zero? x)])"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
