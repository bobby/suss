(assert (= "1.12.6" (clojure-version)))
(try (prn "(letfn [(^double f [] 127.1)] (try (byte (f)) (catch Throwable e :error)))" (eval (read-string "(letfn [(^double f [] 127.1)] (try (byte (f)) (catch Throwable e :error)))"))) (catch Throwable e (prn :error (ex-message e))))
(try (prn "(letfn [(f ^double [] 127.1)] (try (byte (f)) (catch Throwable e :error)))" (eval (read-string "(letfn [(f ^double [] 127.1)] (try (byte (f)) (catch Throwable e :error)))"))) (catch Throwable e (prn :error (ex-message e))))
(try (prn "((fn ^long [] (letfn [(^double f [] 1.5)] (f))))" (eval (read-string "((fn ^long [] (letfn [(^double f [] 1.5)] (f))))"))) (catch Throwable e (prn :error (ex-message e))))
