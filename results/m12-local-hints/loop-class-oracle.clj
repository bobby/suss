(assert (= "1.12.6" (clojure-version)))
(doseq [source ["(let [y (loop [^long x (identity 5.5)] x)] [y (+ y 1)])"
"(loop [x 1.0 n 0] (let [f (fn [] (let [^long y x] y))] (if (zero? n) (recur (identity x) 1) (f))))"
"(loop [x 1.0 n 0] (let [f (fn [] ^long x)] (if (zero? n) (recur (identity x) 1) (f))))"
"(let [^long x (identity 5.5)] (let [y (if true x (long 1))] [y (+ y 1)]))"]] (prn source (try [:value (eval (read-string source))] (catch Throwable e [:error (ex-message e) (some-> e ex-cause ex-message)]))))
