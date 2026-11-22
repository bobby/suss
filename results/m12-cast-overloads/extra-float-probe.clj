(assert (= "1.12.6" (clojure-version)))
(let [source "(let [x (unchecked-float 2147483648.0)] [(int x) (try (int (identity x)) (catch Throwable e :error))])" actual (pr-str (eval (read-string source)))] (prn source actual) (assert (= "[2147483647 :error]" actual)))
(let [source "[(double (float (unchecked-float ##Inf))) (try (float (identity (unchecked-float ##Inf))) (catch Throwable e :error)) (try ((identity float) (unchecked-float ##Inf)) (catch Throwable e :error))]" actual (pr-str (eval (read-string source)))] (prn source actual) (assert (= "[##Inf ##Inf ##Inf]" actual)))
