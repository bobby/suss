(assert (= "1.12.6" (clojure-version)))
(try (prn "(letfn [(f [] 1) (f [] 2)] (f))" (eval (read-string "(letfn [(f [] 1) (f [] 2)] (f))"))) (catch Throwable e (prn :error (ex-message e))))
(try (prn "(letfn* [f 1] f)" (eval (read-string "(letfn* [f 1] f)"))) (catch Throwable e (prn :error (ex-message e))))
(try (prn "(letfn [(^long f [^long x] (+ x 1)) (g [] (f 3))] (g))" (eval (read-string "(letfn [(^long f [^long x] (+ x 1)) (g [] (f 3))] (g))"))) (catch Throwable e (prn :error (ex-message e))))
